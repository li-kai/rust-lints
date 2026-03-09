use clippy_utils::diagnostics::span_lint_and_help;
use clippy_utils::{is_entrypoint_fn, is_in_test};
use rustc_hir::def_id::DefId;
use rustc_hir::{Expr, ExprKind};
use rustc_lint::{LateContext, LateLintPass, LintContext as _};

use crate::config::GlobalSideEffectConfig;

// ── Lint declarations ───────────────────────────────────────────────

rustc_session::declare_lint! {
    /// Flags direct calls to wall-clock or monotonic time functions.
    pub GLOBAL_SIDE_EFFECT_TIME,
    Warn,
    "direct call to a time function \u{2014} accept a time parameter instead"
}

rustc_session::declare_lint! {
    /// Flags direct calls to random number generation functions.
    pub GLOBAL_SIDE_EFFECT_RANDOMNESS,
    Warn,
    "direct call to a random function \u{2014} accept an `impl Rng` parameter instead"
}

rustc_session::declare_lint! {
    /// Flags direct calls to environment variable or CLI argument functions.
    pub GLOBAL_SIDE_EFFECT_ENV,
    Warn,
    "direct call to an environment function \u{2014} pass the value as a parameter instead"
}

// ── Default path lists ──────────────────────────────────────────────

const DEFAULT_TIME_PATHS: &[&str] = &[
    "std::time::SystemTime::now",
    "std::time::Instant::now",
    "chrono::Utc::now",
    "chrono::Local::now",
    "chrono::offset::Utc::now",
    "chrono::offset::Local::now",
    "time::OffsetDateTime::now_utc",
    "time::OffsetDateTime::now_local",
    "time::Instant::now",
    "jiff::Zoned::now",
    "jiff::Timestamp::now",
    "tokio::time::Instant::now",
];

const DEFAULT_RANDOMNESS_PATHS: &[&str] = &[
    "std::random::random",
    // rand 0.8
    "rand::thread_rng",
    // rand 0.9+
    "rand::rng",
    "rand::random",
    "rand::random_range",
    "rand::rngs::OsRng::new",
    "rand::rngs::StdRng::from_os_rng",
    // getrandom
    "getrandom::getrandom",
    // fastrand
    "fastrand::bool",
    "fastrand::u8",
    "fastrand::u16",
    "fastrand::u32",
    "fastrand::u64",
    "fastrand::u128",
    "fastrand::usize",
    "fastrand::i8",
    "fastrand::i16",
    "fastrand::i32",
    "fastrand::i64",
    "fastrand::i128",
    "fastrand::isize",
    "fastrand::f32",
    "fastrand::f64",
    "fastrand::char",
    "fastrand::Rng::new",
];

const DEFAULT_ENV_PATHS: &[&str] = &[
    // std
    "std::env::var",
    "std::env::var_os",
    "std::env::vars",
    "std::env::vars_os",
    "std::env::args",
    "std::env::args_os",
    // dotenvy
    "dotenvy::dotenv",
    "dotenvy::dotenv_override",
    "dotenvy::from_filename",
    "dotenvy::var",
    "dotenvy::vars",
    // dotenv (unmaintained predecessor)
    "dotenv::dotenv",
    "dotenv::var",
    "dotenv::vars",
];

// ── Help messages ───────────────────────────────────────────────────

const HELP_TIME: &str =
    "accept a time parameter or use a clock trait so callers can control the time source in tests";
const HELP_RANDOMNESS: &str = "accept an `impl Rng` parameter so callers can inject a seeded RNG";
const HELP_ENV: &str =
    "move this to your application's entry point and pass the value as a parameter";

// ── Lint pass struct ────────────────────────────────────────────────

/// A single lint pass that checks for all three categories of global side effects.
/// Each category has its own `Lint` and configured path list, but the detection
/// logic is identical: match call expressions against known function paths.
///
/// Chose a single pass over three separate passes to avoid triple-traversing
/// the HIR for what is essentially the same check with different path lists.
pub struct GlobalSideEffect {
    /// Effective path lists after applying config overrides.
    time_paths: Vec<String>,
    randomness_paths: Vec<String>,
    env_paths: Vec<String>,
}

impl GlobalSideEffect {
    pub fn new() -> Self {
        let config: GlobalSideEffectConfig =
            dylint_linting::config_or_default("global_side_effect");

        Self {
            time_paths: build_path_list(DEFAULT_TIME_PATHS, &config.time),
            randomness_paths: build_path_list(DEFAULT_RANDOMNESS_PATHS, &config.randomness),
            env_paths: build_path_list(DEFAULT_ENV_PATHS, &config.env),
        }
    }
}

rustc_session::impl_lint_pass!(GlobalSideEffect => [
    GLOBAL_SIDE_EFFECT_TIME,
    GLOBAL_SIDE_EFFECT_RANDOMNESS,
    GLOBAL_SIDE_EFFECT_ENV,
]);

impl<'tcx> LateLintPass<'tcx> for GlobalSideEffect {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        // Skip macro-generated code.
        if expr.span.from_expansion() {
            return;
        }

        // Resolve the DefId of the called function, if any.
        let Some(def_id) = resolve_callee_def_id(cx, expr) else {
            return;
        };

        // Resolve the full path string once, then compare against all lists.
        let callee_path = cx.tcx.def_path_str(def_id);

        // Determine which category matches (if any).
        let (lint, matched_path, help) =
            if let Some(p) = find_matching_path(&callee_path, &self.time_paths) {
                (&GLOBAL_SIDE_EFFECT_TIME, p, HELP_TIME)
            } else if let Some(p) = find_matching_path(&callee_path, &self.randomness_paths) {
                (&GLOBAL_SIDE_EFFECT_RANDOMNESS, p, HELP_RANDOMNESS)
            } else if let Some(p) = find_matching_path(&callee_path, &self.env_paths) {
                (&GLOBAL_SIDE_EFFECT_ENV, p, HELP_ENV)
            } else {
                return;
            };

        // Check suppression zones only for matched calls (avoids work on
        // the vast majority of expressions that are not flagged).
        if is_in_suppression_zone(cx, expr) {
            return;
        }

        span_lint_and_help(
            cx,
            lint,
            expr.span,
            format!("direct call to `{matched_path}()`"),
            None,
            help,
        );
    }
}

// ── Helper functions ────────────────────────────────────────────────

/// Resolves the `DefId` of the function being called, handling both
/// `ExprKind::Call` (free functions, associated functions) and
/// `ExprKind::MethodCall` (method syntax with receiver).
fn resolve_callee_def_id(cx: &LateContext<'_>, expr: &Expr<'_>) -> Option<DefId> {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "ExprKind has many variants; only Call and MethodCall are relevant"
    )]
    match &expr.kind {
        ExprKind::Call(callee, _) => {
            // For `Foo::bar()` or `some_fn()`, the callee is a path expression.
            if let ExprKind::Path(qpath) = &callee.kind {
                cx.qpath_res(qpath, callee.hir_id).opt_def_id()
            } else {
                None
            }
        }
        ExprKind::MethodCall(..) => {
            // For `receiver.method()`, use typeck to resolve the actual method.
            cx.typeck_results().type_dependent_def_id(expr.hir_id)
        }
        _ => None,
    }
}

/// Returns `true` if the expression is inside a suppression zone:
///
/// - **Test crate** — the crate is compiled with `--test` (integration tests
///   in `tests/`, or `cargo test` on the main crate). Detected via
///   `cx.sess().is_test_crate()`. This covers test helper functions that
///   don't carry `#[test]` themselves (e.g. `tests/common/mod.rs`).
/// - **Test function** — any function registered with the test harness
///   (`#[test]`, `#[tokio::test]`, `#[rstest]`, etc.). Detected via
///   `clippy_utils::is_in_test` which checks for `#[rustc_test_marker]`,
///   covering all proc-macro test attributes automatically.
/// - **`#[cfg(test)]` module** — also handled by `is_in_test`.
/// - **`fn main()`** — the composition root, where wiring up real
///   dependencies is expected.
fn is_in_suppression_zone(cx: &LateContext<'_>, expr: &Expr<'_>) -> bool {
    // Entire crate is a test target (integration tests, `cargo test` build).
    if cx.sess().is_test_crate() {
        return true;
    }

    let hir_id = expr.hir_id;

    // Inside a #[test] function or #[cfg(test)] module.
    if is_in_test(cx.tcx, hir_id) {
        return true;
    }

    // fn main() — the composition root.
    let enclosing_def_id = cx.tcx.hir_enclosing_body_owner(hir_id);
    is_entrypoint_fn(cx, enclosing_def_id.to_def_id())
}

/// Checks if `callee_path` (from `def_path_str`) matches any configured path.
/// Returns the matched path string for use in the diagnostic message.
fn find_matching_path<'a>(callee_path: &str, paths: &'a [String]) -> Option<&'a str> {
    // def_path_str returns e.g. "std::env::var" — direct string comparison.
    paths
        .iter()
        .find(|p| p.as_str() == callee_path)
        .map(String::as_str)
}

/// Builds the effective path list from defaults and config overrides.
/// If `config.paths` is `Some`, it replaces defaults entirely.
/// Otherwise, defaults are merged with `config.additional_paths`.
fn build_path_list(defaults: &[&str], config: &crate::config::SubLintConfig) -> Vec<String> {
    if let Some(ref overrides) = config.paths {
        overrides.clone()
    } else {
        let mut merged: Vec<String> = defaults.iter().map(|&s| s.to_owned()).collect();
        merged.extend(config.additional_paths.iter().cloned());
        merged
    }
}

#[cfg(test)]
mod tests {
    use dylint_testing::ui;

    #[test]
    fn ui_global_side_effect() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "global_side_effect").run();
    }
}

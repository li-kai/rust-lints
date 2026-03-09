use clippy_utils::diagnostics::span_lint_and_help;
use rustc_hir::Expr;
use rustc_lint::{LateContext, LateLintPass};

use super::call_matching::{
    build_path_list, find_matching_path, is_in_suppression_zone, resolve_callee_def_id,
};
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

#[cfg(test)]
mod tests {
    use dylint_testing::ui;

    #[test]
    fn ui_global_side_effect() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "global_side_effect").run();
    }
}

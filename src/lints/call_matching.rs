//! Shared utilities for lints that match function calls against configured path lists.
//!
//! Used by `global_side_effect` and `unbounded_channel`.

use clippy_utils::is_entrypoint_fn;
use rustc_hir::def_id::DefId;
use rustc_hir::{Expr, ExprKind};
use rustc_lint::LateContext;

use super::suppression::is_in_test_zone;
use crate::config::SubLintConfig;

/// Resolves the `DefId` of the function being called, handling both
/// `ExprKind::Call` (free functions, associated functions) and
/// `ExprKind::MethodCall` (method syntax with receiver).
pub fn resolve_callee_def_id(cx: &LateContext<'_>, expr: &Expr<'_>) -> Option<DefId> {
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
/// - **Test zone** — test crate, `#[test]` function, or `#[cfg(test)]` module
///   (see `suppression::is_in_test_zone`).
/// - **`fn main()`** — the composition root, where wiring up real
///   dependencies is expected.
pub fn is_in_suppression_zone(cx: &LateContext<'_>, expr: &Expr<'_>) -> bool {
    if is_in_test_zone(cx, expr) {
        return true;
    }

    // fn main() — the composition root.
    let enclosing_def_id = cx.tcx.hir_enclosing_body_owner(expr.hir_id);
    is_entrypoint_fn(cx, enclosing_def_id.to_def_id())
}

/// Checks if `callee_path` (from `def_path_str`) matches any configured path.
/// Returns the matched path string for use in the diagnostic message.
pub fn find_matching_path<'a>(callee_path: &str, paths: &'a [String]) -> Option<&'a str> {
    // def_path_str returns e.g. "std::env::var" — direct string comparison.
    paths
        .iter()
        .find(|p| p.as_str() == callee_path)
        .map(String::as_str)
}

/// Builds the effective path list from defaults and config overrides.
/// If `config.paths` is `Some`, it replaces defaults entirely.
/// Otherwise, defaults are merged with `config.additional_paths`.
pub fn build_path_list(defaults: &[&str], config: &SubLintConfig) -> Vec<String> {
    if let Some(ref overrides) = config.paths {
        overrides.clone()
    } else {
        let mut merged: Vec<String> = defaults.iter().map(|&s| s.to_owned()).collect();
        merged.extend(config.additional_paths.iter().cloned());
        merged
    }
}

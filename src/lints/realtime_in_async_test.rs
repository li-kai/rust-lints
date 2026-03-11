//! Flags `tokio::time::sleep`, `timeout`, `interval`, and `sleep_until` calls
//! inside test functions that don't have the tokio clock paused.
//!
//! # Detection approach
//!
//! `#[tokio::test]` is a proc macro that expands into a `#[test]` fn wrapping
//! the user's async body in a tokio runtime. The `start_paused = true` variant
//! generates a `.start_paused(true)` call on the runtime builder.
//!
//! We detect this from the expanded code:
//! 1. Find test functions (via `rustc_test_marker` / `is_in_test`).
//! 2. Walk the body for time-related calls (`tokio::time::sleep`, etc.).
//! 3. Walk the body for `.start_paused(true)` — present when the user wrote
//!    `#[tokio::test(start_paused = true)]`.
//! 4. Fire if time calls found but no `start_paused(true)`.
//!
//! This avoids depending on the proc macro's attribute syntax (consumed before
//! HIR) and instead observes the generated code.
//!
//! # Scope
//!
//! - Only fires inside test functions (not production async code).
//! - Only fires on `tokio::time::*` calls, not `std::thread::sleep`
//!   (which is a different problem — see `blocking_in_async`).
//! - Suppressed by `#[allow]`.
//! - Does NOT fire on `tokio::time::advance` (that's the solution, not the
//!   problem).

use clippy_utils::diagnostics::span_lint_and_help;
use clippy_utils::is_in_test;
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{Body, Expr, ExprKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::hir::nested_filter;

use super::call_matching::{build_path_list, find_matching_path, resolve_callee_def_id};
use crate::config::SubLintConfig;

rustc_session::declare_lint! {
    /// Flags real-time waits in async tests that should use a paused clock.
    pub REALTIME_IN_ASYNC_TEST,
    Warn,
    "real-time wait in async test \u{2014} use `#[tokio::test(start_paused = true)]`"
}

const DEFAULT_TIME_PATHS: &[&str] = &[
    "tokio::time::sleep",
    "tokio::time::sleep_until",
    "tokio::time::timeout",
    "tokio::time::timeout_at",
    "tokio::time::interval",
    "tokio::time::interval_at",
];

const HELP: &str = "switch to `#[tokio::test(start_paused = true)]` to resolve \
                     sleeps instantly; use `tokio::time::advance()` for precise control";

pub struct RealtimeInAsyncTest {
    time_paths: Vec<String>,
}

impl RealtimeInAsyncTest {
    pub fn new() -> Self {
        let config: SubLintConfig = dylint_linting::config_or_default("realtime_in_async_test");
        Self {
            time_paths: build_path_list(DEFAULT_TIME_PATHS, &config),
        }
    }
}

rustc_session::impl_lint_pass!(RealtimeInAsyncTest => [REALTIME_IN_ASYNC_TEST]);

impl<'tcx> LateLintPass<'tcx> for RealtimeInAsyncTest {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        _kind: rustc_hir::intravisit::FnKind<'tcx>,
        _decl: &'tcx rustc_hir::FnDecl<'tcx>,
        body: &'tcx Body<'tcx>,
        _span: rustc_span::Span,
        def_id: rustc_hir::def_id::LocalDefId,
    ) {
        // Only top-level test functions (skip closures, async blocks, etc.).
        if !matches!(_kind, rustc_hir::intravisit::FnKind::ItemFn(..)) {
            return;
        }
        let hir_id = cx.tcx.local_def_id_to_hir_id(def_id);
        if !is_in_test(cx.tcx, hir_id) {
            return;
        }

        // Walk the entire body (including proc-macro-generated runtime setup)
        // collecting two signals.
        let mut visitor = TimeCallVisitor {
            cx,
            time_paths: &self.time_paths,
            first_time_call_span: None,
            has_start_paused_true: false,
        };
        intravisit::walk_body(&mut visitor, body);

        let Some(time_span) = visitor.first_time_call_span else {
            return; // No time calls — nothing to flag.
        };

        if visitor.has_start_paused_true {
            return; // Clock is paused — time calls are instant.
        }

        span_lint_and_help(
            cx,
            REALTIME_IN_ASYNC_TEST,
            time_span,
            "real-time wait in async test without paused clock",
            None,
            HELP,
        );
    }
}

/// Walks a function body looking for tokio time calls and `start_paused(true)`.
struct TimeCallVisitor<'a, 'tcx> {
    cx: &'a LateContext<'tcx>,
    time_paths: &'a [String],
    /// Span of the first tokio time call found (for diagnostic pointing).
    first_time_call_span: Option<rustc_span::Span>,
    /// Whether `.start_paused(true)` was found in the body.
    has_start_paused_true: bool,
}

impl<'tcx> Visitor<'tcx> for TimeCallVisitor<'_, 'tcx> {
    type NestedFilter = nested_filter::OnlyBodies;

    fn maybe_tcx(&mut self) -> Self::MaybeTyCtxt {
        self.cx.tcx
    }

    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        // Short-circuit: once we have both signals, the outcome is determined.
        if self.first_time_call_span.is_some() && self.has_start_paused_true {
            return;
        }

        // Check for tokio time calls (only until we find the first one).
        if self.first_time_call_span.is_none()
            && let Some(def_id) = resolve_callee_def_id(self.cx, expr)
        {
            let callee_path = self.cx.tcx.def_path_str(def_id);
            if find_matching_path(&callee_path, self.time_paths).is_some() {
                self.first_time_call_span = Some(expr.span);
            }
        }

        // Check for .start_paused(true) — method call named "start_paused"
        // with a boolean `true` literal argument.
        if !self.has_start_paused_true && is_start_paused_true(expr) {
            self.has_start_paused_true = true;
        }

        intravisit::walk_expr(self, expr);
    }
}

/// Returns `true` if `expr` is a method call `.start_paused(true)`.
fn is_start_paused_true(expr: &Expr<'_>) -> bool {
    if let ExprKind::MethodCall(method, _receiver, args, _span) = &expr.kind
        && method.ident.as_str() == "start_paused"
        && let [arg] = args
        && is_bool_lit_true(arg)
    {
        return true;
    }
    false
}

/// Returns `true` if `expr` is a boolean literal `true`.
const fn is_bool_lit_true(expr: &Expr<'_>) -> bool {
    if let ExprKind::Lit(lit) = &expr.kind {
        matches!(lit.node, rustc_ast::LitKind::Bool(true))
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use dylint_testing::ui;

    #[test]
    fn ui_realtime_in_async_test() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "realtime_in_async_test")
            .rustc_flags(["--test"])
            .run();
    }
}

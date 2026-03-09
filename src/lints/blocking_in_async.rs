use clippy_utils::diagnostics::span_lint_and_help;
use rustc_hir::{ClosureKind, CoroutineDesugaring, CoroutineKind, Expr, ExprKind, Node};
use rustc_lint::{LateContext, LateLintPass};

use super::call_matching::{build_path_list, find_matching_path, resolve_callee_def_id};
use super::suppression::is_in_test_zone;
use crate::config::SubLintConfig;

// ── Lint declaration ────────────────────────────────────────────────

rustc_session::declare_lint! {
    /// Flags known-blocking operations inside `async fn` or `async {}` blocks.
    pub BLOCKING_IN_ASYNC,
    Warn,
    "blocking call inside async context \u{2014} starves the executor"
}

// ── Default paths ───────────────────────────────────────────────────

const DEFAULT_PATHS: &[&str] = &[
    // std::fs
    "std::fs::read",
    "std::fs::read_to_string",
    "std::fs::write",
    "std::fs::read_dir",
    "std::fs::metadata",
    "std::fs::canonicalize",
    "std::fs::copy",
    "std::fs::create_dir",
    "std::fs::create_dir_all",
    "std::fs::remove_file",
    "std::fs::remove_dir",
    "std::fs::remove_dir_all",
    "std::fs::rename",
    // std::net
    "std::net::TcpStream::connect",
    "std::net::TcpListener::bind",
    "std::net::UdpSocket::bind",
    // std::thread
    "std::thread::sleep",
    // std::io — stdin methods are MethodCall, matched by path
    "std::io::Stdin::read_line",
    "std::io::Stdin::read",
    // std::sync
    "std::sync::Mutex::lock",
    "std::sync::RwLock::read",
    "std::sync::RwLock::write",
    // parking_lot
    "parking_lot::Mutex::lock",
    "parking_lot::RwLock::read",
    "parking_lot::RwLock::write",
    // std::thread::spawn — bypasses executor
    "std::thread::spawn",
    // tokio::task::block_in_place — risky on single-threaded executors
    "tokio::task::block_in_place",
];

/// Paths that act as "escape hatches" — if the blocking call is inside a
/// closure passed to one of these, it's intentional.
const SPAWN_BLOCKING_PATHS: &[&str] = &[
    "tokio::task::spawn_blocking",
    "async_std::task::spawn_blocking",
];

const HELP: &str = "use an async-aware alternative, or wrap the blocking call \
                     in `tokio::task::spawn_blocking()`";

// ── Lint pass ───────────────────────────────────────────────────────

pub struct BlockingInAsync {
    paths: Vec<String>,
}

impl BlockingInAsync {
    pub fn new() -> Self {
        let config: SubLintConfig = dylint_linting::config_or_default("blocking_in_async");

        Self {
            paths: build_path_list(DEFAULT_PATHS, &config),
        }
    }
}

rustc_session::impl_lint_pass!(BlockingInAsync => [BLOCKING_IN_ASYNC]);

impl<'tcx> LateLintPass<'tcx> for BlockingInAsync {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        // Skip macro-generated code.
        if expr.span.from_expansion() {
            return;
        }

        let Some(def_id) = resolve_callee_def_id(cx, expr) else {
            return;
        };

        let callee_path = cx.tcx.def_path_str(def_id);

        let Some(matched_path) = find_matching_path(&callee_path, &self.paths) else {
            return;
        };

        // Only lint inside async context.
        if !is_in_async_context(cx, expr) {
            return;
        }

        // Suppress in test zones.
        if is_in_test_zone(cx, expr) {
            return;
        }

        // Suppress inside spawn_blocking escape hatches.
        if is_inside_spawn_blocking(cx, expr) {
            return;
        }

        span_lint_and_help(
            cx,
            BLOCKING_IN_ASYNC,
            expr.span,
            format!("blocking call to `{matched_path}()` inside async context"),
            None,
            HELP,
        );
    }
}

// ── Async context detection ─────────────────────────────────────────

/// Returns `true` if `expr` is syntactically inside an `async fn` or
/// `async {}` block.
///
/// Walks up the HIR parent chain. If we encounter an async fn signature
/// or an async closure/block before hitting a sync function boundary,
/// the expression is in async context.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "we only care about closures and function boundaries"
)]
fn is_in_async_context(cx: &LateContext<'_>, expr: &Expr<'_>) -> bool {
    for (_, node) in cx.tcx.hir_parent_iter(expr.hir_id) {
        match node {
            // async fn/block desugars into a closure with Coroutine(Async) kind.
            Node::Expr(e)
                if matches!(
                    e.kind,
                    ExprKind::Closure(c) if matches!(
                        c.kind,
                        ClosureKind::Coroutine(CoroutineKind::Desugared(
                            CoroutineDesugaring::Async, _,
                        ))
                    )
                ) =>
            {
                return true;
            }
            // Stop at any function boundary — we're in sync context.
            Node::Item(_) | Node::ImplItem(_) | Node::TraitItem(_) => return false,
            _ => {}
        }
    }
    false
}

/// Returns `true` if `expr` is inside a closure passed to
/// `tokio::task::spawn_blocking()` or equivalent escape hatch.
///
/// Walks up the HIR parent chain looking for a closure whose parent
/// is a call to a known `spawn_blocking` function.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "we only care about closures and function boundaries"
)]
fn is_inside_spawn_blocking(cx: &LateContext<'_>, expr: &Expr<'_>) -> bool {
    for (hir_id, node) in cx.tcx.hir_parent_iter(expr.hir_id) {
        match node {
            Node::Expr(Expr {
                kind: ExprKind::Closure(_),
                ..
            }) => {
                // Found a closure ancestor — check if its parent is a
                // call to a known spawn_blocking function.
                if let Node::Expr(parent) = cx.tcx.hir_node(cx.tcx.parent_hir_id(hir_id))
                    && let Some(def_id) = resolve_callee_def_id(cx, parent)
                {
                    let path = cx.tcx.def_path_str(def_id);
                    if SPAWN_BLOCKING_PATHS.iter().any(|&p| p == path) {
                        return true;
                    }
                }
            }
            Node::Item(_) | Node::ImplItem(_) | Node::TraitItem(_) => break,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use dylint_testing::ui;

    #[test]
    fn ui_blocking_in_async() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "blocking_in_async").run();
    }
}

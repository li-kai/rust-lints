use clippy_utils::diagnostics::span_lint_and_then;
use clippy_utils::is_trait_impl_item;
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{Closure, Expr, ExprKind, ImplItem, ImplItemKind, LangItem, Node};
use rustc_lint::{LateContext, LateLintPass};
use rustc_span::{ExpnKind, Span, sym};

rustc_session::declare_lint! {
    /// Warns when a `Drop::drop` implementation contains operations that can
    /// panic, since panicking during unwinding causes an immediate process abort.
    pub PANIC_IN_DROP,
    Deny,
    "panic-able expression in `Drop` impl \u{2014} this will abort during unwinding"
}

pub struct PanicInDrop;

impl PanicInDrop {
    pub const fn new() -> Self {
        Self
    }
}

rustc_session::impl_lint_pass!(PanicInDrop => [PANIC_IN_DROP]);

impl<'tcx> LateLintPass<'tcx> for PanicInDrop {
    fn check_impl_item(&mut self, cx: &LateContext<'tcx>, impl_item: &'tcx ImplItem<'tcx>) {
        let ImplItemKind::Fn(_sig, body_id) = &impl_item.kind else {
            return;
        };

        // Skip macro-generated impls, and must be a drop impl
        if impl_item.span.from_expansion()
            || impl_item.ident.as_str() != "drop"
            // Fast pre-check: avoids HIR parent walk for inherent impls
            || !is_trait_impl_item(cx, impl_item.hir_id())
            || !is_drop_impl(cx, impl_item)
        {
            return;
        }

        let body = cx.tcx.hir_body(*body_id);
        let typeck = cx.tcx.typeck(impl_item.owner_id.def_id);
        let mut finder = DropPanicFinder {
            cx,
            typeck,
            inside_panicking_guard: false,
            findings: Vec::new(),
        };
        intravisit::walk_body(&mut finder, body);

        if finder.findings.is_empty() {
            return;
        }

        span_lint_and_then(
            cx,
            PANIC_IN_DROP,
            impl_item.span,
            "panic-able expression in `Drop` impl \u{2014} this will abort during unwinding",
            |diag| {
                for (span, desc) in &finder.findings {
                    diag.span_note(
                        *span,
                        format!(
                            "`{desc}` can panic — handle the error or use \
                             `let _ = ...` to ignore it"
                        ),
                    );
                }
                diag.help(
                    "panicking in `drop()` while already unwinding causes an \
                     immediate process abort",
                );
            },
        );
    }
}

/// Returns `true` if the parent `impl` block is `impl Drop for T`.
fn is_drop_impl<'tcx>(cx: &LateContext<'tcx>, impl_item: &'tcx ImplItem<'tcx>) -> bool {
    // Chose trait DefId comparison over string matching because it works
    // regardless of imports or re-exports.
    let parent_id = cx.tcx.parent_hir_id(impl_item.hir_id());
    let Node::Item(item) = cx.tcx.hir_node(parent_id) else {
        return false;
    };
    let rustc_hir::ItemKind::Impl(impl_block) = &item.kind else {
        return false;
    };
    let Some(trait_header) = impl_block.of_trait else {
        return false;
    };
    let Some(trait_def_id) = trait_header.trait_ref.trait_def_id() else {
        return false;
    };
    cx.tcx.is_lang_item(trait_def_id, LangItem::Drop)
}

struct DropPanicFinder<'a, 'tcx> {
    cx: &'a LateContext<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    /// When true, we're inside an `if std::thread::panicking()` guard —
    /// skip findings there since the author already handles double-panic.
    inside_panicking_guard: bool,
    /// Collected (span, description) pairs for each panicking expression found.
    findings: Vec<(Span, &'static str)>,
}

/// Returns `true` if the receiver type of a method call is `Option` or `Result`.
fn receiver_is_option_or_result<'tcx>(
    cx: &LateContext<'tcx>,
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    receiver: &Expr<'tcx>,
) -> bool {
    let ty = typeck.expr_ty_adjusted(receiver).peel_refs();
    if let rustc_middle::ty::Adt(adt, _) = ty.kind() {
        let did = adt.did();
        return cx.tcx.is_diagnostic_item(sym::Option, did)
            || cx.tcx.is_diagnostic_item(sym::Result, did);
    }
    false
}

// No NestedFilter — deliberately skip closures and async blocks.
// A closure stored in a field or passed to a callback doesn't panic during drop.
impl<'tcx> Visitor<'tcx> for DropPanicFinder<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        // Skip closure/async block bodies — panics there don't run during drop
        if matches!(expr.kind, ExprKind::Closure(Closure { .. })) || self.inside_panicking_guard {
            return;
        }

        // Detect `if std::thread::panicking()` or `if !std::thread::panicking()`
        // and suppress findings in the branch that only runs during unwinding.
        if let ExprKind::If(cond, then_branch, else_branch) = &expr.kind
            && is_panicking_guard(self.cx, cond)
        {
            // The author has guarded with `panicking()` — both branches
            // are safe from double-panic: one runs only when unwinding,
            // the other only when not. Suppress findings in both.
            self.inside_panicking_guard = true;
            intravisit::walk_expr(self, then_branch);
            if let Some(else_branch) = else_branch {
                intravisit::walk_expr(self, else_branch);
            }
            self.inside_panicking_guard = false;
            return;
        }

        // Check for panic macros: panic!, unreachable!, assert!, assert_eq!, assert_ne!
        // (checked before method calls to avoid double-reporting macro internals)
        if expr.span.from_expansion()
            && let Some((call_site, desc)) = find_panic_macro(expr.span)
        {
            self.findings.push((call_site, desc));
            return;
        }

        if let ExprKind::MethodCall(method, receiver, _args, span) = &expr.kind {
            let name = method.ident.as_str();
            if (name == "unwrap" || name == "expect")
                && receiver_is_option_or_result(self.cx, self.typeck, receiver)
            {
                let desc = if name == "unwrap" {
                    ".unwrap()"
                } else {
                    ".expect()"
                };
                self.findings.push((*span, desc));
            }
        }

        intravisit::walk_expr(self, expr);
    }
}

/// Checks if a span originates from a panic-related macro, walking up the
/// expansion chain to handle cases like `panic!` expanding through internal
/// macros (`panic_fmt`, `panic_2021`, etc.).
fn find_panic_macro(span: Span) -> Option<(Span, &'static str)> {
    let mut sp = span;
    loop {
        let expn_data = sp.ctxt().outer_expn_data();
        if let ExpnKind::Macro(_, macro_name) = &expn_data.kind {
            let desc: Option<&'static str> = match macro_name.as_str() {
                "panic" => Some("panic!()"),
                "unreachable" => Some("unreachable!()"),
                "assert" => Some("assert!()"),
                "assert_eq" => Some("assert_eq!()"),
                "assert_ne" => Some("assert_ne!()"),
                _ => None,
            };
            if let Some(desc) = desc {
                return Some((expn_data.call_site, desc));
            }
            // Walk up to the parent expansion (e.g. panic_fmt -> panic)
            let parent = expn_data.call_site;
            if parent.ctxt() == sp.ctxt() || !parent.from_expansion() {
                return None;
            }
            sp = parent;
        } else {
            return None;
        }
    }
}

/// Returns `true` if the expression is `std::thread::panicking()` or
/// `!std::thread::panicking()`.
fn is_panicking_guard<'tcx>(cx: &LateContext<'tcx>, cond: &Expr<'tcx>) -> bool {
    let inner = if let ExprKind::Unary(rustc_hir::UnOp::Not, inner) = &cond.kind {
        inner
    } else {
        cond
    };

    if let ExprKind::Call(callee, _) = &inner.kind
        && let ExprKind::Path(qpath) = &callee.kind
        && let Some(def_id) = cx.qpath_res(qpath, callee.hir_id).opt_def_id()
    {
        return cx.tcx.def_path_str(def_id) == "std::thread::panicking";
    }
    false
}

#[cfg(test)]
mod tests {
    use dylint_testing::ui;

    #[test]
    fn ui_panic_in_drop() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "panic_in_drop").run();
    }
}

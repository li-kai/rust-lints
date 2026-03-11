use clippy_utils::diagnostics::span_lint_and_then;
use clippy_utils::is_trait_impl_item;
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{Closure, Expr, ExprKind, ImplItem, ImplItemKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty;
use rustc_span::{ExpnKind, Span, sym};

use crate::config::FallibleNewConfig;

// ── Lint declaration ────────────────────────────────────────────────

rustc_session::declare_lint! {
    /// Warns when a `fn new()` constructor contains operations that can panic,
    /// suggesting it return `Result` or be renamed to convey fallibility.
    pub FALLIBLE_NEW,
    Deny,
    "constructor `new` can panic \u{2014} consider returning `Result` or renaming to `try_new`"
}

pub struct FallibleNew {
    check_new_variants: bool,
}

impl FallibleNew {
    pub fn new() -> Self {
        let config: FallibleNewConfig = dylint_linting::config_or_default("fallible_new");
        Self {
            check_new_variants: config.check_new_variants,
        }
    }
}

rustc_session::impl_lint_pass!(FallibleNew => [FALLIBLE_NEW]);

impl<'tcx> LateLintPass<'tcx> for FallibleNew {
    fn check_impl_item(&mut self, cx: &LateContext<'tcx>, impl_item: &'tcx ImplItem<'tcx>) {
        // Only check functions
        let ImplItemKind::Fn(_sig, body_id) = &impl_item.kind else {
            return;
        };

        // Skip macro-generated impls
        if impl_item.span.from_expansion() {
            return;
        }

        let name = impl_item.ident.as_str();

        // Check method name: "new" or "new_*" variants
        if name != "new" && !(self.check_new_variants && name.starts_with("new_")) {
            return;
        }

        // Skip non-public methods — private constructors are internal invariants
        if !is_sufficiently_visible(cx, impl_item) {
            return;
        }

        // Skip trait impls — signature is dictated by the trait
        if is_trait_impl_item(cx, impl_item.hir_id()) {
            return;
        }

        // Skip if return type is already Result
        if returns_result(cx, impl_item) {
            return;
        }

        // Walk the body looking for panicking expressions
        let body = cx.tcx.hir_body(*body_id);
        let mut finder = PanicFinder {
            cx,
            findings: Vec::new(),
        };
        intravisit::walk_body(&mut finder, body);

        if finder.findings.is_empty() {
            return;
        }

        span_lint_and_then(
            cx,
            FALLIBLE_NEW,
            impl_item.span,
            format!(
                "constructor `{name}` can panic \u{2014} consider returning `Result` or renaming to `try_{name}`"
            ),
            |diag| {
                for (span, desc) in &finder.findings {
                    diag.span_note(
                        *span,
                        format!("`{desc}` can panic \u{2014} use `?` with a `Result` return type instead"),
                    );
                }
            },
        );
    }
}

// ── Skip-condition helpers ──────────────────────────────────────────

/// Returns `true` if the method is `pub` or `pub(crate)`.
fn is_sufficiently_visible<'tcx>(cx: &LateContext<'tcx>, impl_item: &'tcx ImplItem<'tcx>) -> bool {
    // Use effective visibility: lint pub and pub(crate), skip private/pub(super).
    let def_id = impl_item.owner_id.def_id;
    cx.tcx.effective_visibilities(()).is_reachable(def_id)
}

/// Returns `true` if the function's return type is `Result<_, _>`.
fn returns_result<'tcx>(cx: &LateContext<'tcx>, impl_item: &'tcx ImplItem<'tcx>) -> bool {
    // Use the type-checked return type to handle type aliases
    // like `type MyResult<T> = Result<T, MyError>`.
    let def_id = impl_item.owner_id.to_def_id();
    let fn_sig = cx.tcx.fn_sig(def_id).instantiate_identity();
    let ret_ty = fn_sig.output().skip_binder();
    if let ty::Adt(adt, _) = ret_ty.kind() {
        return cx.tcx.is_diagnostic_item(sym::Result, adt.did());
    }
    false
}

/// Returns `true` if the receiver type of a method call is `Option` or `Result`.
fn receiver_is_option_or_result<'tcx>(cx: &LateContext<'tcx>, receiver: &Expr<'tcx>) -> bool {
    let ty = cx.typeck_results().expr_ty_adjusted(receiver).peel_refs();
    if let ty::Adt(adt, _) = ty.kind() {
        let did = adt.did();
        return cx.tcx.is_diagnostic_item(sym::Option, did)
            || cx.tcx.is_diagnostic_item(sym::Result, did);
    }
    false
}

// ── Body visitor: find panicking expressions ────────────────────────

struct PanicFinder<'a, 'tcx> {
    cx: &'a LateContext<'tcx>,
    /// Collected (span, description) pairs for each panicking expression found.
    findings: Vec<(Span, &'static str)>,
}

// No NestedFilter — deliberately skip closures and async blocks.
// A closure stored in a field or returned doesn't panic during construction.
impl<'tcx> Visitor<'tcx> for PanicFinder<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        // Skip closure/async block bodies — panics there don't run during construction
        if matches!(expr.kind, ExprKind::Closure(Closure { .. })) {
            return;
        }

        // Check for .unwrap() and .expect() on Option/Result
        if let ExprKind::MethodCall(method, receiver, _args, span) = &expr.kind {
            let name = method.ident.as_str();
            if (name == "unwrap" || name == "expect")
                && receiver_is_option_or_result(self.cx, receiver)
            {
                let desc = if name == "unwrap" {
                    ".unwrap()"
                } else {
                    ".expect()"
                };
                self.findings.push((*span, desc));
            }
        }

        // Check for panic macros: panic!, unreachable!
        if expr.span.from_expansion() {
            let expn_data = expr.span.ctxt().outer_expn_data();
            if let ExpnKind::Macro(_, macro_name) = expn_data.kind {
                let desc: Option<&'static str> = match macro_name.as_str() {
                    "panic" => Some("panic!()"),
                    "unreachable" => Some("unreachable!()"),
                    _ => None,
                };
                if let Some(desc) = desc {
                    self.findings.push((expn_data.call_site, desc));
                    // Don't walk into macro expansion
                    return;
                }
            }
        }

        intravisit::walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use dylint_testing::ui;

    #[test]
    fn ui_fallible_new() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "fallible_new").run();
    }
}

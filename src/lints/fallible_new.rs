use clippy_utils::diagnostics::span_lint_and_then;
use clippy_utils::is_trait_impl_item;
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{Closure, Expr, ExprKind, ImplItem, ImplItemKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty;
use rustc_span::{Span, sym};

use super::hir_refs;
use crate::config::FallibleNewConfig;

rustc_session::declare_lint! {
    /// Warns when a `fn new()` constructor contains operations that can panic,
    /// suggesting it return `Result` or be renamed to convey fallibility.
    pub FALLIBLE_NEW,
    Deny,
    "constructor `new` can panic \u{2014} consider returning `Result` or renaming to `try_new`"
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

struct PanicFinder {
    /// Collected (span, description) pairs for each panicking expression found.
    findings: Vec<(Span, &'static str)>,
}

// No NestedFilter — deliberately skip closures and async blocks.
// A closure stored in a field or returned doesn't panic during construction.
impl Visitor<'_> for PanicFinder {
    fn visit_expr(&mut self, expr: &'_ Expr<'_>) {
        // Skip closure/async block bodies — panics there don't run during construction
        if matches!(expr.kind, ExprKind::Closure(Closure { .. })) {
            return;
        }

        if let ExprKind::MethodCall(method, _receiver, _args, span) = &expr.kind {
            let name = method.ident.as_str();
            if name == "unwrap" || name == "expect" {
                let desc = if name == "unwrap" { ".unwrap()" } else { ".expect()" };
                self.findings.push((*span, desc));
            }
        }

        if expr.span.from_expansion() {
            if let Some((call_site, kind)) = hir_refs::find_panic_macro(expr.span) {
                if matches!(kind, hir_refs::PanicMacro::Panic | hir_refs::PanicMacro::Unreachable)
                {
                    self.findings.push((call_site, kind.desc()));
                }
                // Don't walk into any panic-family macro expansion
                return;
            }
        }

        intravisit::walk_expr(self, expr);
    }
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
        let ImplItemKind::Fn(_sig, body_id) = &impl_item.kind else {
            return;
        };

        if impl_item.span.from_expansion() {
            return;
        }

        let name = impl_item.ident.as_str();

        if name != "new" && !(self.check_new_variants && name.starts_with("new_")) {
            return;
        }

        // Skip trait impls (signature dictated by trait).
        if is_trait_impl_item(cx, impl_item.hir_id()) || returns_result(cx, impl_item) {
            return;
        }

        let body = cx.tcx.hir_body(*body_id);
        let mut finder = PanicFinder {
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

#[cfg(test)]
mod tests {
    #[test]
    fn ui_fallible_new() {
        crate::testing::run_ui_test("fallible_new", None, &[]);
    }
}

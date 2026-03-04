use clippy_utils::diagnostics::span_lint_and_help;
use rustc_hir::{Item, ItemKind, VariantData};
use rustc_lint::{LateContext, LateLintPass};

use crate::config::NeedlessBuilderConfig;
use super::has_bon_builder_derive;

rustc_session::declare_lint! {
    /// Warns when `bon::Builder` is derived on a struct with very few fields.
    pub NEEDLESS_BUILDER,
    Warn,
    "warns when `bon::Builder` is used on structs with few fields"
}

pub struct NeedlessBuilder {
    threshold: usize,
}

impl NeedlessBuilder {
    pub fn new() -> Self {
        let config: NeedlessBuilderConfig =
            dylint_linting::config_or_default(env!("CARGO_PKG_NAME"));
        Self {
            threshold: config.threshold,
        }
    }
}

rustc_session::impl_lint_pass!(NeedlessBuilder => [NEEDLESS_BUILDER]);

impl<'tcx> LateLintPass<'tcx> for NeedlessBuilder {
    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        if item.span.from_expansion() {
            return;
        }
        let ItemKind::Struct(variant_data, _) = &item.kind else {
            return;
        };
        let VariantData::Struct { fields, .. } = variant_data else {
            return;
        };
        let field_count = fields.len();
        if field_count > self.threshold {
            return;
        }
        let attrs = cx.tcx.hir_attrs(item.hir_id());
        if !has_bon_builder_derive(attrs) {
            return;
        }
        span_lint_and_help(
            cx,
            NEEDLESS_BUILDER,
            item.span,
            format!(
                "struct `{}` has only {field_count} fields; `bon::Builder` may be unnecessary",
                item.ident,
            ),
            None,
            "consider using a plain constructor or struct literal instead",
        );
    }
}

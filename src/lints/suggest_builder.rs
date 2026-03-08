use clippy_utils::diagnostics::span_lint_and_help;
use rustc_hir::{Item, ItemKind, VariantData};
use rustc_lint::{LateContext, LateLintPass};

use crate::config::SuggestBuilderConfig;

rustc_session::declare_lint! {
    /// Suggests adding `#[derive(bon::Builder)]` to structs with many named fields.
    pub SUGGEST_BUILDER,
    Warn,
    "suggests adding `#[derive(bon::Builder)]` to structs with many fields"
}

pub struct SuggestBuilder {
    threshold: usize,
}

impl SuggestBuilder {
    pub fn new() -> Self {
        let config: SuggestBuilderConfig = dylint_linting::config_or_default("suggest_builder");
        Self {
            threshold: config.threshold,
        }
    }
}

rustc_session::impl_lint_pass!(SuggestBuilder => [SUGGEST_BUILDER]);

impl<'tcx> LateLintPass<'tcx> for SuggestBuilder {
    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        if item.span.from_expansion() {
            return;
        }
        let ItemKind::Struct(ident, _, variant_data) = &item.kind else {
            return;
        };
        let VariantData::Struct { fields, .. } = variant_data else {
            return;
        };
        let field_count = fields.len();
        if field_count < self.threshold {
            return;
        }
        if super::has_bon_builder(ident.name) {
            return;
        }
        span_lint_and_help(
            cx,
            SUGGEST_BUILDER,
            item.span,
            format!("struct `{ident}` has {field_count} fields but does not derive `bon::Builder`",),
            None,
            "add `#[derive(bon::Builder)]` to enable the builder pattern",
        );
    }
}

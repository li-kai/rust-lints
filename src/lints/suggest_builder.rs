use clippy_utils::diagnostics::span_lint_and_help;
use rustc_hir::{GenericParamKind, Item, ItemKind, LangItem, VariantData};
use rustc_lint::{LateContext, LateLintPass};
use rustc_span::Symbol;

use crate::config::SuggestBuilderConfig;

rustc_session::declare_lint! {
    /// Suggests adding `#[derive(bon::Builder)]` to structs with many named fields.
    pub SUGGEST_BUILDER,
    Warn,
    "suggests adding `#[derive(bon::Builder)]` to structs with many fields"
}

pub struct SuggestBuilder {
    threshold: usize,
    skip_derives: Vec<Symbol>,
}

impl SuggestBuilder {
    pub fn new() -> Self {
        let config: SuggestBuilderConfig = dylint_linting::config_or_default("suggest_builder");
        Self {
            threshold: config.threshold,
            skip_derives: config
                .skip_derives
                .iter()
                .map(|s| Symbol::intern(s))
                .collect(),
        }
    }
}

rustc_session::impl_lint_pass!(SuggestBuilder => [SUGGEST_BUILDER]);

impl<'tcx> LateLintPass<'tcx> for SuggestBuilder {
    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        if item.span.from_expansion() {
            return;
        }
        let ItemKind::Struct(ident, generics, variant_data) = &item.kind else {
            return;
        };
        // Skip structs with lifetime parameters — they represent borrowed
        // views, visitors, or traversal contexts where a builder is
        // structurally inappropriate.
        if generics
            .params
            .iter()
            .any(|p| matches!(p.kind, GenericParamKind::Lifetime { .. }))
        {
            return;
        }
        // Skip `#[repr(C)]` structs — layout is dictated by FFI, a builder
        // is structurally inappropriate.
        let adt_def = cx.tcx.adt_def(item.owner_id);
        if adt_def.repr().c() {
            return;
        }
        let VariantData::Struct { fields, .. } = variant_data else {
            return;
        };
        // Don't count `PhantomData` fields (including variance markers like
        // `PhantomData<*const T>`, `PhantomData<fn(T)>`, etc.) — they aren't
        // real from a construction-ergonomics standpoint.
        let field_count = fields
            .iter()
            .filter(|f| {
                let ty = cx.tcx.type_of(f.def_id).instantiate_identity();
                !ty.ty_adt_def()
                    .is_some_and(|adt| cx.tcx.is_lang_item(adt.did(), LangItem::PhantomData))
            })
            .count();
        if field_count < self.threshold {
            return;
        }
        if super::has_bon_builder(ident.name) {
            return;
        }
        // Skip structs named `*Builder` — they ARE builders, not builder
        // candidates.
        if ident.name.as_str().ends_with("Builder") {
            return;
        }
        // Skip structs that derive any trait in the configured `skip_derives`
        // list (default: Default, Queryable, Insertable, Selectable).
        if super::has_any_derive(ident.name, &self.skip_derives) {
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

#[cfg(test)]
mod tests {
    use dylint_testing::ui;

    #[test]
    fn ui_suggest_builder() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "suggest_builder")
            .dylint_toml("[suggest_builder]\nthreshold = 4\n[needless_builder]\nthreshold = 0\n")
            .run();
    }
}

use clippy_utils::diagnostics::span_lint_and_help;
use rustc_hir::{Item, ItemKind, VariantData};
use rustc_lint::{LateContext, LateLintPass};

use crate::config::LargeStructConfig;

rustc_session::declare_lint! {
    /// Warns when a struct has an excessive number of fields.
    pub LARGE_STRUCT,
    Warn,
    "warns when a struct has an excessive number of fields"
}

pub struct LargeStruct {
    threshold: usize,
}

impl LargeStruct {
    pub fn new() -> Self {
        let config: LargeStructConfig =
            dylint_linting::config_or_default("large_struct");
        Self {
            threshold: config.threshold,
        }
    }
}

rustc_session::impl_lint_pass!(LargeStruct => [LARGE_STRUCT]);

impl<'tcx> LateLintPass<'tcx> for LargeStruct {
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
        span_lint_and_help(
            cx,
            LARGE_STRUCT,
            item.span,
            format!(
                "struct `{}` has {field_count} fields, consider splitting into smaller types",
                ident,
            ),
            None,
            "group related fields into separate structs to improve readability",
        );
    }
}

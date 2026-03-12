use rustc_ast::{Item, ItemKind, MetaItemInner};
use rustc_lint::{EarlyContext, EarlyLintPass};
use rustc_span::symbol::{Symbol, sym};

use super::{BON_BUILDER_STRUCTS, STRUCT_DERIVES};

rustc_session::declare_lint! {
    /// Internal lint used to collect derive information from structs.
    ///
    /// Runs as a pre-expansion `EarlyLintPass` so it can inspect derive
    /// attributes before they are consumed by macro expansion.  Populates:
    /// - [`BON_BUILDER_STRUCTS`](super::BON_BUILDER_STRUCTS) for structs
    ///   deriving `bon::Builder`
    /// - [`STRUCT_DERIVES`](super::STRUCT_DERIVES) with all derive trait names
    ///   per struct (used by `suggest_builder`'s `skip_derives` config)
    ///
    /// See [`has_bon_builder`](super::has_bon_builder) for the implications
    /// of name-only matching.
    pub BON_BUILDER_COLLECTOR,
    Allow,
    "internal: collects structs deriving bon::Builder"
}

pub struct BonBuilderCollector;

rustc_session::impl_lint_pass!(BonBuilderCollector => [BON_BUILDER_COLLECTOR]);

impl EarlyLintPass for BonBuilderCollector {
    fn check_item(&mut self, _cx: &EarlyContext<'_>, item: &Item) {
        let ItemKind::Struct(ident, ..) = &item.kind else {
            return;
        };
        let derives = collect_derive_names(&item.attrs);
        if derives.is_empty() {
            return;
        }
        let builder_sym = Symbol::intern("Builder");
        if derives.contains(&builder_sym) {
            BON_BUILDER_STRUCTS.with(|set| {
                set.borrow_mut().insert(ident.name);
            });
        }
        STRUCT_DERIVES.with(|map| {
            map.borrow_mut()
                .entry(ident.name)
                .or_default()
                .extend(derives);
        });
    }
}

/// Extracts the last path segment of every trait in `#[derive(...)]` attributes.
fn collect_derive_names(attrs: &[rustc_ast::Attribute]) -> Vec<Symbol> {
    let mut names = Vec::new();
    for attr in attrs {
        if !attr.has_name(sym::derive) {
            continue;
        }
        let Some(list) = attr.meta_item_list() else {
            continue;
        };
        for item in list {
            let MetaItemInner::MetaItem(meta) = item else {
                continue;
            };
            if let Some(last) = meta.path.segments.last() {
                names.push(last.ident.name);
            }
        }
    }
    names
}

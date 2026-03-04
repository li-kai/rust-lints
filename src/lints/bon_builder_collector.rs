use rustc_ast::{Item, ItemKind, MetaItemInner};
use rustc_lint::{EarlyContext, EarlyLintPass};
use rustc_span::symbol::{sym, Symbol};

use super::BON_BUILDER_STRUCTS;

rustc_session::declare_lint! {
    /// Internal lint used to collect structs with `#[derive(bon::Builder)]`.
    ///
    /// Runs as a pre-expansion `EarlyLintPass` so it can inspect derive
    /// attributes before they are consumed by macro expansion.  Populates the
    /// [`BON_BUILDER_STRUCTS`](super::BON_BUILDER_STRUCTS) set by struct
    /// *name* (`Symbol`).  See [`has_bon_builder`](super::has_bon_builder) for
    /// the implications of name-only matching.
    pub BON_BUILDER_COLLECTOR,
    Allow,
    "internal: collects structs deriving bon::Builder"
}

pub struct BonBuilderCollector;

rustc_session::impl_lint_pass!(BonBuilderCollector => [BON_BUILDER_COLLECTOR]);

impl EarlyLintPass for BonBuilderCollector {
    fn check_item(&mut self, _cx: &EarlyContext<'_>, item: &Item) {
        if let ItemKind::Struct(ident, ..) = &item.kind {
            if has_bon_builder_derive_ast(&item.attrs) {
                BON_BUILDER_STRUCTS.with(|set| {
                    set.borrow_mut().insert(ident.name);
                });
            }
        }
    }
}

fn has_bon_builder_derive_ast(attrs: &[rustc_ast::Attribute]) -> bool {
    let builder_sym = Symbol::intern("Builder");
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
            let segs: Vec<Symbol> = meta
                .path
                .segments
                .iter()
                .map(|s| s.ident.name)
                .collect();
            match segs.as_slice() {
                [_, b] if *b == builder_sym => return true,
                [b] if *b == builder_sym => return true,
                _ => {}
            }
        }
    }
    false
}

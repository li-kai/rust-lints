pub mod large_struct;
pub mod needless_builder;
pub mod suggest_builder;

use rustc_ast::Attribute;
use rustc_span::symbol::sym;

/// Returns `true` if any attribute in `attrs` is a `#[derive(...)]` containing
/// `bon::Builder` or `Builder`.
pub fn has_bon_builder_derive(attrs: &[Attribute]) -> bool {
    for attr in attrs {
        if !attr.has_name(sym::derive) {
            continue;
        }
        let Some(list) = attr.meta_item_list() else {
            continue;
        };
        for item in &list {
            let Some(meta) = item.meta_item() else {
                continue;
            };
            let segments: Vec<&str> = meta
                .path
                .segments
                .iter()
                .map(|s| s.ident.name.as_str())
                .collect();
            match segments.as_slice() {
                [_, "Builder"] | ["Builder"] => return true,
                _ => {}
            }
        }
    }
    false
}

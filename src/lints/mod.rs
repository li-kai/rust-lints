pub mod blocking_in_async;
pub mod bon_builder_collector;
pub mod call_matching;
pub mod debug_remnants;
pub mod fallible_new;
pub mod global_side_effect;
pub mod map_init_then_insert;
pub mod needless_builder;
pub mod proper_error_type;
pub mod suggest_builder;
mod suppression;
pub mod unbounded_channel;

use core::cell::RefCell;
use std::collections::HashSet;

use rustc_span::Symbol;

thread_local! {
    pub static BON_BUILDER_STRUCTS: RefCell<HashSet<Symbol>> = RefCell::new(HashSet::new());
}

/// Returns `true` if a struct with the given name was found to have
/// `#[derive(bon::Builder)]` during the pre-expansion pass.
///
/// **Limitation:** This uses name-only matching (not path or `DefId`) because
/// the pre-expansion AST pass runs before name resolution.  If two structs in
/// different modules share the same name and only one derives `bon::Builder`,
/// both will be treated as having (or not having) the derive.  This can cause
/// false positives in `needless_builder` and false negatives in
/// `suggest_builder`.  Switching to a `LateLintPass` with `DefId`-based
/// matching would fix this, but at the cost of not seeing derives that are
/// consumed by macro expansion before the HIR is built.
pub fn has_bon_builder(name: Symbol) -> bool {
    BON_BUILDER_STRUCTS.with(|set| set.borrow().contains(&name))
}

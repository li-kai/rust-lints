pub mod acyclic_modules;
pub mod blocking_in_async;
pub mod bon_builder_collector;
pub mod call_matching;
pub mod debug_remnants;
pub mod fallible_new;
pub mod global_side_effect;
mod hir_refs;
pub mod map_init_then_insert;
pub mod module_dependencies;
pub mod await_holding_unsendable;
pub mod needless_builder;
pub mod panic_in_drop;
pub mod proper_error_type;
pub mod realtime_in_async_test;
pub mod result_result;
pub mod suggest_builder;
mod suppression;
pub mod topological_ordering;
pub mod unbounded_channel;
pub mod unclear_exports;
pub mod unsafe_send_missing_drop;
pub mod unstructured_log_fields;

use core::cell::RefCell;
use std::collections::{HashMap, HashSet};

use rustc_span::Symbol;

thread_local! {
    /// Maps struct names to the set of derive trait names found on them during
    /// the pre-expansion pass.  Populated by [`BonBuilderCollector`] and
    /// consumed via [`has_any_derive`] / [`has_bon_builder`].
    pub static STRUCT_DERIVES: RefCell<HashMap<Symbol, HashSet<Symbol>>> = RefCell::new(HashMap::new());
}

/// Returns `true` if any of the given derive names were found on a struct
/// with the given name during the pre-expansion pass.
pub fn has_any_derive(name: Symbol, derives: &[Symbol]) -> bool {
    STRUCT_DERIVES.with(|map| {
        map.borrow()
            .get(&name)
            .is_some_and(|set| derives.iter().any(|d| set.contains(d)))
    })
}

/// Returns `true` if a struct with the given name was found to have
/// `#[derive(bon::Builder)]` during the pre-expansion pass.
///
/// **Limitation:** Uses name-only matching (not path or `DefId`) because the
/// pre-expansion AST pass runs before name resolution.  If two structs in
/// different modules share the same name and only one derives `bon::Builder`,
/// both will be treated identically.  Switching to a `LateLintPass` would fix
/// this at the cost of not seeing derives consumed by macro expansion.
pub fn has_bon_builder(name: Symbol) -> bool {
    has_any_derive(name, &[Symbol::intern("Builder")])
}

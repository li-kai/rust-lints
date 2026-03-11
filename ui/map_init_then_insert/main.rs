#![allow(
    unknown_lints,
    dead_code,
    debug_remnants,
    unused_variables,
    unused_mut,
    clippy::allow_attributes_without_reason
)]
// Tests for the `map_init_then_insert` lint.

use ahash::AHashMap;
use indexmap::IndexMap;
use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, HashMap};

// ══════════════════════════════════════════════════════════════════════
// Should trigger: HashMap::new() + ≥2 inserts
// ══════════════════════════════════════════════════════════════════════

fn hashmap_new_two_inserts() {
    let mut m = HashMap::new(); //~ WARNING: immediately inserting into a newly created map
    m.insert("a", 1);
    m.insert("b", 2);
}

fn hashmap_new_three_inserts() {
    let mut m = HashMap::new(); //~ WARNING: immediately inserting into a newly created map
    m.insert("a", 1);
    m.insert("b", 2);
    m.insert("c", 3);
}

// ══════════════════════════════════════════════════════════════════════
// Should trigger: BTreeMap::new() + ≥2 inserts
// ══════════════════════════════════════════════════════════════════════

fn btreemap_new_two_inserts() {
    let mut m = BTreeMap::new(); //~ WARNING: immediately inserting into a newly created map
    m.insert(1, "one");
    m.insert(2, "two");
}

// ══════════════════════════════════════════════════════════════════════
// Should trigger: IndexMap::new() + ≥2 inserts
// ══════════════════════════════════════════════════════════════════════

fn indexmap_new_two_inserts() {
    let mut m = IndexMap::new(); //~ WARNING: immediately inserting into a newly created map
    m.insert("a", 1);
    m.insert("b", 2);
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: IndexMap with only one insert
// ══════════════════════════════════════════════════════════════════════

fn indexmap_one_insert_only() {
    let mut m = IndexMap::new();
    m.insert("a", 1);
}

// ══════════════════════════════════════════════════════════════════════
// Should trigger: ::default() constructor
// ══════════════════════════════════════════════════════════════════════

fn hashmap_default_two_inserts() {
    let mut m: HashMap<&str, i32> = HashMap::default(); //~ WARNING: immediately inserting into a newly created map
    m.insert("x", 10);
    m.insert("y", 20);
}

// ══════════════════════════════════════════════════════════════════════
// Should trigger: ::with_capacity() constructor
// ══════════════════════════════════════════════════════════════════════

fn hashmap_with_capacity_two_inserts() {
    let mut m = HashMap::with_capacity(4); //~ WARNING: immediately inserting into a newly created map
    m.insert("a", 1);
    m.insert("b", 2);
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: only one insert (below MIN_INSERTS threshold)
// ══════════════════════════════════════════════════════════════════════

fn one_insert_only() {
    let mut m = HashMap::new();
    m.insert("a", 1);
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: intervening non-insert statement
// ══════════════════════════════════════════════════════════════════════

fn intervening_statement() {
    let mut m = HashMap::new();
    m.insert("a", 1);
    println!("inserted a");
    m.insert("b", 2);
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: control flow between creation and inserts
// ══════════════════════════════════════════════════════════════════════

fn control_flow_between() {
    let mut m = HashMap::new();
    if true {
        m.insert("a", 1);
    }
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: map is read between inserts
// ══════════════════════════════════════════════════════════════════════

fn read_between_inserts() {
    let mut m = HashMap::new();
    m.insert("a", 1);
    let _v = m.get("a");
    m.insert("b", 2);
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: already using `from`
// ══════════════════════════════════════════════════════════════════════

fn already_from() {
    let m = HashMap::from([("a", 1), ("b", 2)]);
}

// ══════════════════════════════════════════════════════════════════════
// Should trigger: map used after the init sequence (from is still better)
// ══════════════════════════════════════════════════════════════════════

fn used_after_sequence() {
    let mut m = HashMap::new(); //~ WARNING: immediately inserting into a newly created map
    m.insert("a", 1);
    m.insert("b", 2);
    // Even though the map is used later, `from` still wins on readability
    // and allocation. The `mut` keyword is one keyword, not a reason to
    // suppress the lint.
    process(&m);
    m.insert("c", 3);
}

fn process(_m: &HashMap<&str, i32>) {}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: macro-generated code
// ══════════════════════════════════════════════════════════════════════

macro_rules! make_map {
    () => {{
        let mut m = HashMap::new();
        m.insert("a", 1);
        m.insert("b", 2);
        m
    }};
}

fn macro_generated() {
    let _m = make_map!();
}

// ══════════════════════════════════════════════════════════════════════
// Should trigger: FxHashMap (type alias of HashMap via rustc-hash)
// ══════════════════════════════════════════════════════════════════════

fn fxhashmap_default_two_inserts() {
    let mut m = FxHashMap::default(); //~ WARNING: immediately inserting into a newly created map
    m.insert("a", 1);
    m.insert("b", 2);
}

// ══════════════════════════════════════════════════════════════════════
// Should trigger: AHashMap (type alias of HashMap via ahash)
// ══════════════════════════════════════════════════════════════════════

fn ahashmap_new_two_inserts() {
    let mut m = AHashMap::new(); //~ WARNING: immediately inserting into a newly created map
    m.insert("a", 1);
    m.insert("b", 2);
}

fn main() {}

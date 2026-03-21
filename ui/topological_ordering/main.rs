// Test cases for the `topological_ordering` lint.
//
// Default mode: callee-first (referenced items should appear before
// items that reference them).
#![allow(dead_code, unknown_lints, unused_variables)]

// ── Case 1: Correct callee-first order ─────────────────────────────
// No warnings expected: helper is defined before process, which is
// defined before run.

mod correct_order {
    fn helper() -> u32 {
        42
    }

    fn process() -> u32 {
        helper() + 1
    }

    fn run() {
        let _ = process();
    }
}

// ── Case 2: Incorrect order (caller before callee) ─────────────────
// Warning expected: `caller` appears before `callee`, but `caller`
// references `callee`.

mod wrong_order {
    fn caller() {
        callee(); //~ WARN items are not in topological order
    }

    fn callee() {}
}

// ── Case 3: Type reference in function signature ───────────────────
// Warning expected: `process` references `Config` but appears before it.

mod type_before_fn {
    fn process(_cfg: Config) {} //~ WARN items are not in topological order

    struct Config {
        value: u32,
    }
}

// ── Case 4: Correct order with struct and function ─────────────────
// No warnings expected: Config is defined before process.

mod type_correct {
    struct Config {
        value: u32,
    }

    fn process(_cfg: Config) {}
}

// ── Case 5: Inherent impl adjacent to struct (correct) ─────────────
// No warnings expected.

mod impl_adjacent {
    struct Widget {
        name: String,
    }

    impl Widget {
        fn new(name: String) -> Self {
            Self { name }
        }
    }

    fn use_widget() {
        let _ = Widget::new("test".into());
    }
}

// ── Case 6: Inherent impl separated from struct ────────────────────
// Warning expected: `impl Widget` is separated from `struct Widget`
// by `fn unrelated`.

mod impl_separated {
    struct Widget {
        name: String,
    }

    fn unrelated() -> u32 {
        0
    }

    impl Widget {
        //~^ WARN `impl Widget` is separated from its type definition
        fn new(name: String) -> Self {
            Self { name }
        }
    }
}

// ── Case 7: Trait impl not required adjacent ───────────────────────
// No warnings expected: trait impls are separate items, not grouped
// with the type.

mod trait_impl_separate {
    struct Point {
        x: f64,
        y: f64,
    }

    fn compute(p: Point) -> f64 {
        p.x + p.y
    }

    impl std::fmt::Display for Point {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "({}, {})", self.x, self.y)
        }
    }
}

// ── Case 8: Mutual recursion (cycle) ───────────────────────────────
// No warnings expected for the two functions relative to each other
// (they form an SCC).  But the SCC should still be ordered relative
// to other items.

mod mutual_recursion {
    fn leaf() -> bool {
        true
    }

    // parse_expr and parse_atom form an SCC -- no warning between them.
    fn parse_expr(depth: usize) -> u32 {
        if leaf() {
            parse_atom(depth + 1)
        } else {
            0
        }
    }

    fn parse_atom(depth: usize) -> u32 {
        if depth < 10 {
            parse_expr(depth + 1)
        } else {
            1
        }
    }
}

// ── Case 9: Constant referenced by function ────────────────────────
// Warning expected: `use_max` references `MAX_SIZE` but appears before it.

mod const_ordering {
    fn use_max() -> usize {
        MAX_SIZE //~ WARN items are not in topological order
    }

    const MAX_SIZE: usize = 1024;
}

// ── Case 10: Correct constant order ────────────────────────────────
// No warnings expected.

mod const_correct {
    const MAX_SIZE: usize = 1024;

    fn use_max() -> usize {
        MAX_SIZE
    }
}

// ── Case 11: Type alias in signature ───────────────────────────────
// Warning expected: `process` references `Id` but appears before it.

mod type_alias_order {
    fn process(_id: Id) {} //~ WARN items are not in topological order

    type Id = u64;
}

// ── Case 12: #[allow] suppression ──────────────────────────────────
// No warnings expected due to allow attribute.

#[allow(topological_ordering)]
mod suppressed {
    fn caller() {
        callee();
    }

    fn callee() {}
}

// ── Case 13: Test module excluded ──────────────────────────────────
// No warnings expected: test modules are excluded from analysis.

mod has_tests {
    fn helper() -> u32 {
        42
    }

    #[cfg(test)]
    mod tests {
        // References to items in any order -- no warnings.
        fn test_helper() {
            let _ = super::helper();
        }
    }
}

// ── Case 14: Cross-module references ignored ───────────────────────
// No warnings expected: references to items in other modules do not
// create ordering edges.

mod cross_module_a {
    pub fn shared() -> u32 {
        0
    }
}

mod cross_module_b {
    // This references cross_module_a::shared, but that is a cross-module
    // reference and should not affect ordering within cross_module_b.
    fn uses_external() -> u32 {
        crate::cross_module_a::shared()
    }

    fn local_helper() -> u32 {
        1
    }
}

// ── Case 15: Items with no local references ────────────────────────
// No warnings expected: unconstrained items can appear anywhere.

mod no_refs {
    fn alpha() -> u32 {
        0
    }

    fn beta() -> u32 {
        1
    }

    fn gamma() -> u32 {
        2
    }
}

// ── Case 16: Multi-item SCC with outside dependency ────────────────
// The SCC {a, b, c} should appear after `leaf` (which they all call).
// Warning expected if SCC appears before `leaf`.

mod scc_with_dep {
    fn a() {
        b();
        leaf(); //~ WARN items are not in topological order
    }

    fn b() {
        c();
    }

    fn c() {
        a();
    }

    fn leaf() {}
}

// ── Case 17: Enum with inherent impl ───────────────────────────────
// Same grouping rules apply to enums.

mod enum_grouping {
    enum Color {
        Red,
        Green,
        Blue,
    }

    impl Color {
        fn is_primary(&self) -> bool {
            matches!(self, Self::Red | Self::Green | Self::Blue)
        }
    }

    fn use_color() -> bool {
        Color::Red.is_primary()
    }
}

fn main() {}

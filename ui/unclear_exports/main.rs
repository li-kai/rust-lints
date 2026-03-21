#![allow(dead_code, unknown_lints, unused_imports)]

fn main() {}

// ══════════════════════════════════════════════════════════════════════
// SHOULD TRIGGER — glob imports
// ══════════════════════════════════════════════════════════════════════

mod glob_triggers {
    mod inner {
        pub struct Foo;
        pub struct Bar;
    }

    mod prelude {
        pub struct Qux;
    }

    // -- Private glob import --
    use inner::*;
    //~^ ERROR: glob imports (`use foo::*`) are banned

    // -- pub(crate) glob import --
    pub(crate) use inner::*;
    //~^ ERROR: glob imports (`use foo::*`) are banned

    // -- pub(super) glob import --
    pub(super) use inner::*;
    //~^ ERROR: glob imports (`use foo::*`) are banned

    // -- Fully public glob import --
    pub use inner::*;
    //~^ ERROR: glob imports (`use foo::*`) are banned

    // -- Prelude is NOT exempt --
    use prelude::*;
    //~^ ERROR: glob imports (`use foo::*`) are banned
}

// ══════════════════════════════════════════════════════════════════════
// SHOULD TRIGGER — renamed imports
// ══════════════════════════════════════════════════════════════════════

mod rename_triggers {
    mod inner {
        pub struct Foo;
        pub struct Bar;
    }

    // -- Private rename --
    use inner::Foo as MyFoo;
    //~^ ERROR: renamed imports (`use foo::Bar as Baz`) are banned

    // -- Public rename --
    pub use inner::Bar as MyBar;
    //~^ ERROR: renamed imports (`use foo::Bar as Baz`) are banned

    // -- pub(crate) rename --
    pub(crate) use inner::Foo as CrateFoo;
    //~^ ERROR: renamed imports (`use foo::Bar as Baz`) are banned
}

// ══════════════════════════════════════════════════════════════════════
// SHOULD NOT TRIGGER
// ══════════════════════════════════════════════════════════════════════

mod no_trigger {
    mod inner {
        pub struct Foo;
        pub struct Bar;
    }

    // -- Explicit list import --
    use inner::{Foo, Bar};

    // -- Single item import --
    use inner::Foo as _;

    // -- Explicit pub re-export with original names --
    pub use inner::{Foo as _, Bar as _};
}

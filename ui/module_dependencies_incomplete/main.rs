// A normal compilation sees one feature/target cfg slice. The configured
// source → target permission is unused here, but that does not prove it is
// dead in another supported configuration.
#![allow(dead_code, unknown_lints, topological_ordering)]

mod source {
    pub fn work() {}
}

mod target {
    pub struct Dependency;
}

fn main() {}

#![allow(dead_code, unknown_lints, unused_variables, unused_imports)]

// ══════════════════════════════════════════════════════════════════════
// SHOULD TRIGGER
// ══════════════════════════════════════════════════════════════════════

mod triggers {
    use std::collections::HashMap;

    // -- unwrap in new --
    struct Config {
        data: String,
    }

    impl Config {
        pub fn new(path: &str) -> Self {
            //~^ WARNING: constructor `new` can panic
            let contents = std::fs::read_to_string(path).unwrap();
            Self { data: contents }
        }
    }

    // -- expect in new --
    struct DbPool {
        url: String,
    }

    impl DbPool {
        pub fn new(url: &str) -> Self {
            //~^ WARNING: constructor `new` can panic
            let validated = url.parse::<u16>().expect("invalid port");
            Self {
                url: url.to_string(),
            }
        }
    }

    // -- panic! macro in new --
    struct StrictConfig;

    impl StrictConfig {
        pub fn new(mode: &str) -> Self {
            //~^ WARNING: constructor `new` can panic
            if mode != "strict" {
                panic!("only strict mode is supported");
            }
            Self
        }
    }

    // -- todo! in new --
    struct Placeholder;

    impl Placeholder {
        pub fn new() -> Self {
            //~^ WARNING: constructor `new` can panic
            todo!()
        }
    }

    // -- new_* variant with unwrap --
    struct Server {
        port: u16,
    }

    impl Server {
        pub fn new_with_port(port_str: &str) -> Self {
            //~^ WARNING: constructor `new_with_port` can panic
            let port = port_str.parse::<u16>().unwrap();
            Self { port }
        }
    }

    // -- multiple panicking operations --
    struct Multi {
        a: String,
        b: u16,
    }

    impl Multi {
        pub fn new(a: &str, b: &str) -> Self {
            //~^ WARNING: constructor `new` can panic
            let a = a.parse::<String>().unwrap();
            let b = b.parse::<u16>().expect("bad b");
            Self { a, b }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// SHOULD NOT TRIGGER
// ══════════════════════════════════════════════════════════════════════

mod no_trigger {
    // -- Returns Result — already fallible --
    struct FallibleConfig;

    impl FallibleConfig {
        pub fn new(path: &str) -> Result<Self, std::io::Error> {
            let _contents = std::fs::read_to_string(path)?;
            Ok(Self)
        }
    }

    // -- No fallible operations --
    struct Point {
        x: f64,
        y: f64,
    }

    impl Point {
        pub fn new(x: f64, y: f64) -> Self {
            Self { x, y }
        }
    }

    // -- Private constructor --
    struct Inner;

    impl Inner {
        fn new() -> Self {
            let _val = "42".parse::<u32>().unwrap();
            Self
        }
    }

    // -- Trait impl — signature dictated by trait --
    trait Builder {
        fn new() -> Self;
    }

    struct MyBuilder;

    impl Builder for MyBuilder {
        fn new() -> Self {
            let _x = "oops".parse::<u32>().unwrap();
            Self
        }
    }

    // -- Named try_new — signals fallibility --
    struct TryServer;

    impl TryServer {
        pub fn try_new(addr: &str) -> Result<Self, std::io::Error> {
            Ok(Self)
        }
    }

    // -- Closure with unwrap stored in field — does not panic during construction --
    struct WithCallback {
        cb: Box<dyn Fn() -> u32>,
    }

    impl WithCallback {
        pub fn new() -> Self {
            Self {
                cb: Box::new(|| "42".parse::<u32>().unwrap()),
            }
        }
    }

    // -- Method named "new" but not a constructor (free function, not impl) --
    // (This lint only checks impl items, so standalone fns are out of scope.)
}

fn main() {
    // Entry point for the UI test example.
}

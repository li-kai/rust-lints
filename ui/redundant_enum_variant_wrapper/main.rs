#![allow(
    dead_code,
    unknown_lints,
    unused_braces,
    unused_variables,
    unused_imports,
    topological_ordering
)]

fn main() {}

// SHOULD TRIGGER

mod triggers {
    enum Message<T> {
        Text(String),
        Pair(T, usize),
        Error { code: u16, detail: String },
        Quit,
    }

    impl<T> Message<T> {
        fn text(value: String) -> Self {
            //~^ ERROR: associated function `text` only wraps enum variant `Text`
            Self::Text(value)
        }

        pub fn pair(value: T, index: usize) -> Self {
            //~^ ERROR: associated function `pair` only wraps enum variant `Pair`
            Self::Pair(value, index)
        }

        pub fn error(detail: String, code: u16) -> Self {
            //~^ ERROR: associated function `error` only wraps enum variant `Error`
            Self::Error { code, detail }
        }

        pub fn quit() -> Self {
            //~^ ERROR: associated function `quit` only wraps enum variant `Quit`
            { return Self::Quit }
        }

        // An unrelated associated function does not make construction complex.
        pub fn variant_count() -> usize {
            4
        }

        // Receiver methods are not constructors and do not exempt the enum.
        pub fn unchanged(self) -> Self {
            self
        }
    }

    // Not every variant needs a helper for the enum's constructor API to be
    // entirely simple.
    enum Partial {
        Wrapped(String),
        Bare,
    }

    impl Partial {
        fn wrapped(value: String) -> Self {
            //~^ ERROR: associated function `wrapped` only wraps enum variant `Wrapped`
            Self::Wrapped(value)
        }
    }
}

// SHOULD NOT TRIGGER

mod no_trigger {
    // A trait can require this shape, so implementations are exempt.
    enum TraitMessage {
        Text(String),
    }

    trait FromText {
        fn text(value: String) -> Self;
    }

    impl FromText for TraitMessage {
        fn text(value: String) -> Self {
            Self::Text(value)
        }
    }

    // A helper on another type is outside this lint's scope.
    struct Factory;
    enum Event {
        Quit,
    }

    impl Factory {
        fn quit() -> Event {
            Event::Quit
        }
    }

    // A meaningful constructor for one variant justifies a consistent named
    // constructor API for the enum's simple variants, even across impl blocks.
    enum Request {
        Header(String),
        Parsed(u16),
    }

    impl Request {
        fn header(value: String) -> Self {
            Self::Header(value)
        }
    }

    impl Request {
        fn parsed(value: &str) -> Self {
            Self::Parsed(value.parse().expect("valid request code"))
        }
    }

    // Fallible constructors also make the enum's construction API nontrivial.
    enum Config {
        Name(String),
        Port(u16),
    }
    type ConfigResult = Result<Config, std::num::ParseIntError>;

    impl Config {
        fn name(value: String) -> Self {
            Self::Name(value)
        }

        fn port(value: &str) -> ConfigResult {
            Ok(Self::Port(value.parse()?))
        }
    }

    // A direct-looking implicit coercion is construction behavior, so the
    // sibling wrapper is retained for API consistency.
    enum Bytes {
        Owned(Vec<u8>),
        Static(&'static [u8]),
    }

    impl Bytes {
        fn owned(value: Vec<u8>) -> Self {
            Self::Owned(value)
        }

        fn static_slice(value: &'static [u8; 4]) -> Self {
            Self::Static(value)
        }
    }

    // Enum-wide analysis still honors item-level lint attributes.
    enum Compatible {
        Text(String),
    }

    impl Compatible {
        #[expect(
            redundant_enum_variant_wrapper,
            reason = "retained for compatibility with the version 1 API"
        )]
        fn text(value: String) -> Self {
            Self::Text(value)
        }
    }

    // Impl-level expectations remain attached when the enum owns analysis.
    enum ImplCompatible {
        Text(String),
    }

    #[expect(
        redundant_enum_variant_wrapper,
        reason = "retained for compatibility with the version 1 API"
    )]
    impl ImplCompatible {
        fn text(value: String) -> Self {
            Self::Text(value)
        }
    }

    // A macro-generated constructor conservatively exempts the whole enum.
    macro_rules! generated {
        ($value:expr) => {
            Self::Generated($value)
        };
    }

    enum MacroApi {
        Plain(String),
        Generated(String),
    }

    impl MacroApi {
        fn plain(value: String) -> Self {
            Self::Plain(value)
        }

        fn generated(value: String) -> Self {
            generated!(value)
        }
    }
}

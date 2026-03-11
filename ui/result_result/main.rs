#![allow(dead_code, unknown_lints, unused_variables, unused_imports)]

fn main() {}

use std::io;

// ══════════════════════════════════════════════════════════════════════
// SHOULD TRIGGER
// ══════════════════════════════════════════════════════════════════════

mod triggers {
    use std::io;

    #[derive(Debug)]
    struct Config;
    #[derive(Debug)]
    struct ValidationError;
    #[derive(Debug)]
    struct ParseError;
    #[derive(Debug)]
    struct DecodeError;
    #[derive(Debug)]
    struct Data;

    // -- Nested Result in return type --
    fn parse_and_validate(input: &str) -> Result<Result<Config, ValidationError>, ParseError> {
        //~^ WARNING: nested `Result<Result<_, _>, _>`
        todo!()
    }

    // -- Nested Result in type alias --
    type LoadResult = Result<Result<Data, DecodeError>, io::Error>;
    //~^ WARNING: nested `Result<Result<_, _>, _>`

    // -- Produced by .map() with a fallible closure --
    fn load(path: &str) -> Result<Result<String, std::num::ParseIntError>, io::Error> {
        //~^ WARNING: nested `Result<Result<_, _>, _>`
        std::fs::read_to_string(path).map(|s| s.parse::<i32>().map(|n| n.to_string()))
    }

    // -- Public method --
    struct Loader;
    impl Loader {
        pub fn load(&self) -> Result<Result<String, DecodeError>, io::Error> {
            //~^ WARNING: nested `Result<Result<_, _>, _>`
            todo!()
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// SHOULD NOT TRIGGER
// ══════════════════════════════════════════════════════════════════════

mod no_trigger {
    use std::error::Error;
    use std::io;

    #[derive(Debug)]
    struct Config;
    #[derive(Debug)]
    struct AppError;

    // -- Flat Result with unified error --
    fn parse_and_validate(input: &str) -> Result<Config, AppError> {
        todo!()
    }

    // -- Result with non-Result Ok type --
    fn fetch(url: &str) -> Result<String, io::Error> {
        todo!()
    }

    // -- Generic T that happens to be Result at some call site --
    fn wrap<T>(value: T) -> Result<T, io::Error> {
        Ok(value)
    }

    // -- Trait impl method (signature dictated by trait) --
    trait Parser {
        fn parse(&self) -> Result<Result<String, io::Error>, io::Error>;
    }
    struct MyParser;
    impl Parser for MyParser {
        fn parse(&self) -> Result<Result<String, io::Error>, io::Error> {
            todo!()
        }
    }

    // -- Nested Option (not our business) --
    fn maybe_maybe(x: i32) -> Option<Option<i32>> {
        Some(Some(x))
    }

    // -- Result<Option<_>, _> is fine --
    fn result_option() -> Result<Option<String>, io::Error> {
        Ok(None)
    }

    // -- Type alias that is not nested --
    type SimpleResult = Result<String, io::Error>;
}

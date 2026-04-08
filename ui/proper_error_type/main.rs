#![allow(
    dead_code,
    unknown_lints,
    clippy::allow_attributes_without_reason,
    topological_ordering
)]
// Tests for the `proper_error_type` lint.

use std::fmt;

// Step 1 — Unstructured error types

// Should trigger: Result<_, String>
pub fn parse_string(_input: &str) -> Result<(), String> {
    Ok(())
}

// Should trigger: Result<_, &str>
pub fn parse_str(_input: &str) -> Result<(), &'static str> {
    Ok(())
}

// Should trigger: Result<_, Box<dyn Error>>
pub fn parse_boxed(_input: &str) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

// Should trigger: Result<_, Box<dyn Error + Send + Sync>>
pub fn parse_boxed_send_sync(_input: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Ok(())
}

// Should trigger: pub(crate) with unstructured error
pub(crate) fn pub_crate_string_err(_path: &str) -> Result<(), String> {
    Ok(())
}

// Should NOT trigger: private function
fn private_parse(_input: &str) -> Result<(), String> {
    Ok(())
}

// Should NOT trigger: pub(super) is narrower than pub(crate)
mod step1_pub_super {
    mod inner {
        pub(super) fn pub_super_string_err(_input: &str) -> Result<(), String> {
            Ok(())
        }
    }
}

// Should NOT trigger: typed error
pub fn typed_parse(_input: &str) -> Result<(), MyTypedError> {
    Err(MyTypedError)
}

#[derive(Debug)]
pub struct MyTypedError;
impl fmt::Display for MyTypedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "typed error")
    }
}
impl std::error::Error for MyTypedError {}

// Step 2 — Missing source()

// Should trigger: has io::Error field but no source()
#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(_) => write!(f, "config io error"),
        }
    }
}
impl std::error::Error for ConfigError {}

// Should NOT trigger step 2 (source() implemented) or step 3 (Display
// does not render the inner error).
#[derive(Debug)]
pub enum ConfigErrorWithSource {
    Io(std::io::Error),
}
impl fmt::Display for ConfigErrorWithSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(_) => write!(f, "config io error"),
        }
    }
}
impl std::error::Error for ConfigErrorWithSource {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
        }
    }
}

// Should NOT trigger step 2: no fields that implement Error
#[derive(Debug)]
pub enum SimpleError {
    MissingField(&'static str),
}
impl fmt::Display for SimpleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(name) => write!(f, "missing field: {name}"),
        }
    }
}
impl std::error::Error for SimpleError {}

// Should trigger step 2: pub(crate) type with error field but no source()
#[derive(Debug)]
pub(crate) enum PubCrateConfigError {
    Io(std::io::Error),
}
impl fmt::Display for PubCrateConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(_) => write!(f, "pub(crate) config io error"),
        }
    }
}
impl std::error::Error for PubCrateConfigError {}

// Step 3 — Duplicated source in Display
// (Negative case covered by ConfigErrorWithSource above.)

// Should trigger: Display renders inner error that source() also returns
#[derive(Debug)]
pub enum DupSourceError {
    Io(std::io::Error),
}
impl fmt::Display for DupSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
        }
    }
}
impl std::error::Error for DupSourceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
        }
    }
}

// Step 4 — Manual Error + Display  (no dedicated cases needed; every
// type above with hand-written Error + Display impls triggers step 4.)
//
// Step 5 — *Error without Error impl

// Should trigger: named *Error but doesn't implement Error
pub enum ParseError {
    InvalidSyntax,
    UnexpectedEof,
}

// Should trigger: named *Err but doesn't implement Error
pub struct ConnectionErr {
    pub message: String,
}

// Should NOT trigger: implements Error (MyTypedError above)

// Should NOT trigger: not named *Error
pub enum ParseProblem {
    InvalidSyntax,
}

// Should trigger step 5: pub(crate) type named *Error without Error impl
pub(crate) enum PubCrateParseError {
    InvalidSyntax,
}

// Should NOT trigger: private type
enum InternalError {
    Oops,
}

// Should NOT trigger step 5: pub(super) is narrower than pub(crate)
mod step5_pub_super {
    mod inner {
        pub(super) enum PubSuperErr {
            Oops,
        }
    }
}

// thiserror — should NOT trigger any step

// Should NOT trigger step 4 (manual Error+Display) or step 2 (missing source):
// thiserror generates both impls via proc macro; their spans are from_expansion().
#[derive(thiserror::Error, Debug)]
pub enum ThiserrorError {
    #[error("io failed")]
    Io(#[from] std::io::Error),
}

// Should NOT trigger step 3 (duplicated source): #[error(transparent)]
// intentionally forwards both Display and source() to the inner error.
#[derive(thiserror::Error, Debug)]
pub enum TransparentError {
    #[error(transparent)]
    Io(std::io::Error),
}

// Should NOT trigger step 5 (*Error without Error impl):
// thiserror generates impl Error, so implements_trait returns true.
#[derive(thiserror::Error, Debug)]
pub enum ThiserrorNamedError {
    #[error("invalid")]
    Invalid,
}

fn main() {}

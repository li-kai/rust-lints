#![allow(dead_code, unknown_lints, clippy::allow_attributes_without_reason)]
// Tests for the `proper_error_type` lint.

use std::fmt;

// ══════════════════════════════════════════════════════════════════════
// Step 1 — Unstructured error types
// ══════════════════════════════════════════════════════════════════════

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

// Should NOT trigger: private function
fn private_parse(_input: &str) -> Result<(), String> {
    Ok(())
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

// ══════════════════════════════════════════════════════════════════════
// Step 2 — Missing source()
// ══════════════════════════════════════════════════════════════════════

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

// ══════════════════════════════════════════════════════════════════════
// Step 3 — Duplicated source in Display
// (Negative case covered by ConfigErrorWithSource above.)
// ══════════════════════════════════════════════════════════════════════

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

// ══════════════════════════════════════════════════════════════════════
// Step 4 — Manual Error + Display  (no dedicated cases needed; every
// type above with hand-written Error + Display impls triggers step 4.)
//
// Step 5 — *Error without Error impl
// ══════════════════════════════════════════════════════════════════════

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

// Should NOT trigger: private type
enum InternalError {
    Oops,
}

fn main() {}

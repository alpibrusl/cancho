//! The refusals of the repository layer (`docs/vcs-cli.md` section 9). Every
//! one carries a tag that is the same on every run, so an agent can branch
//! on it; an environment failure (the disk, not the request) is told apart
//! because the CLI gives it another exit code.

use std::fmt;

use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct Refusal {
    pub tag: &'static str,
    pub message: String,
    /// Extra machine-readable fields (`ref-moved` carries the winner's id).
    pub detail: Value,
    /// True for a failure of the machine (I/O), false for a refusal of the request.
    pub environment: bool,
}

pub type Result<T> = std::result::Result<T, Refusal>;

impl Refusal {
    pub fn new(tag: &'static str, message: impl Into<String>) -> Refusal {
        Refusal { tag, message: message.into(), detail: json!({}), environment: false }
    }

    pub fn with(mut self, detail: Value) -> Refusal {
        self.detail = detail;
        self
    }

    pub fn io(context: impl fmt::Display, e: &std::io::Error) -> Refusal {
        Refusal {
            tag: "io-error",
            message: format!("{context}: {e}"),
            detail: json!({}),
            environment: true,
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.tag, self.message)
    }
}

impl std::error::Error for Refusal {}

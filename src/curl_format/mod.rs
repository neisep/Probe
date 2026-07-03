//! Parse a `curl …` command line into a [`RequestDraft`].
//!
//! The fastest onboarding trick for an HTTP client: paste a curl command and
//! get a fully-populated request. This module mirrors the `.http` importer
//! (`crate::http_format`) — a self-contained, side-effect-free `text →
//! RequestDraft` boundary. The UI surfaces it via
//! `PanelIntent::ImportCurlAsRequest`; parse errors are shown to the user, so
//! this module never panics.

pub mod parser;
pub mod tokenizer;

use std::fmt;

pub use parser::{looks_like_curl, parse_curl};

/// A cURL command that could not be parsed into a request.
#[derive(Debug, PartialEq, Eq)]
pub enum CurlParseError {
    /// The command was empty / only whitespace.
    Empty,
    /// The first token was not `curl`.
    NotCurl,
    /// A quote was opened but never closed.
    UnterminatedQuote,
    /// No URL argument was found.
    MissingUrl,
}

impl fmt::Display for CurlParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CurlParseError::Empty => write!(f, "curl command is empty"),
            CurlParseError::NotCurl => write!(f, "not a curl command"),
            CurlParseError::UnterminatedQuote => write!(f, "unterminated quote in curl command"),
            CurlParseError::MissingUrl => write!(f, "curl command has no URL"),
        }
    }
}

impl std::error::Error for CurlParseError {}

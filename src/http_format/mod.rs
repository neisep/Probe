pub mod parser;
pub mod writer;

use std::fmt;

pub use parser::parse_request;
pub use writer::write_request;

#[derive(Debug)]
pub enum HttpFormatError {
    Empty,
    MissingRequestLine,
    MalformedRequestLine(String),
    MalformedHeader(String),
}

impl fmt::Display for HttpFormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpFormatError::Empty => write!(f, "http file is empty"),
            HttpFormatError::MissingRequestLine => write!(f, "http file has no request line"),
            HttpFormatError::MalformedRequestLine(line) => {
                write!(f, "malformed request line: {line}")
            }
            HttpFormatError::MalformedHeader(line) => write!(f, "malformed header: {line}"),
        }
    }
}

impl std::error::Error for HttpFormatError {}

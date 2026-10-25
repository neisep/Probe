//! Multi-request `.http` / `.rest` file parsing.
//!
//! Probe's own on-disk format stores one request per file, so
//! [`parse_request`](super::parse_request) reads a single request. Files
//! written by other tools commonly pack several requests into one file,
//! separated by `###`. This module splits such a file and delegates each
//! chunk to the existing single-request parser.

use crate::state::RequestDraft;

use super::HttpFormatError;
use super::parser::{is_separator, parse_request};

/// Parse every request in a `.http`/`.rest` file.
///
/// Chunks that carry no request line — a trailing `###`, a block of
/// comments — are skipped rather than failing the whole file. A file with
/// no parsable request at all yields [`HttpFormatError::MissingRequestLine`].
pub fn parse_requests(text: &str) -> Result<Vec<RequestDraft>, HttpFormatError> {
    if text.trim().is_empty() {
        return Err(HttpFormatError::Empty);
    }

    let mut requests = Vec::new();
    for chunk in split_chunks(text) {
        if !has_request_line(&chunk) {
            continue;
        }
        requests.push(parse_request(&chunk)?);
    }

    if requests.is_empty() {
        return Err(HttpFormatError::MissingRequestLine);
    }
    Ok(requests)
}

/// Split on `###` separator lines, keeping each request's own comment
/// directives with it.
fn split_chunks(text: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for line in text.lines() {
        if is_separator(line.trim_start()) {
            chunks.push(current.join("\n"));
            current.clear();
            continue;
        }
        current.push(line);
    }
    chunks.push(current.join("\n"));
    chunks
}

/// True when the chunk holds at least one line that is neither blank nor a
/// comment — i.e. something `parse_request` can read as a request line.
fn has_request_line(chunk: &str) -> bool {
    chunk.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("//")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_two_requests_on_separator() {
        let text = "\
# @name First
GET https://example.com/one
Accept: application/json

###

# @name Second
POST https://example.com/two

{\"a\":1}
";
        let requests = parse_requests(text).expect("two requests");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].method, "GET");
        assert_eq!(requests[0].url, "https://example.com/one");
        assert_eq!(requests[0].name, "First");
        assert_eq!(requests[1].method, "POST");
        assert_eq!(requests[1].url, "https://example.com/two");
        assert_eq!(requests[1].body.as_deref(), Some("{\"a\":1}"));
    }

    #[test]
    fn single_request_file_yields_one_draft() {
        let requests = parse_requests("GET https://example.com/only").expect("one request");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, "https://example.com/only");
    }

    #[test]
    fn skips_trailing_separator_and_comment_only_chunks() {
        let text = "\
GET https://example.com/one

###
# just a note, no request here
###
";
        let requests = parse_requests(text).expect("one request");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, "https://example.com/one");
    }

    #[test]
    fn reads_back_a_file_written_by_probe() {
        // Probe writes one request per file via `write_request`; re-importing
        // such a file must yield exactly that request.
        let mut draft = crate::state::RequestDraft::default_request();
        draft.set_request_name("Round trip");
        draft.method = "POST".to_owned();
        draft.set_url("https://example.com/things");
        draft.body = Some("{\"ok\":true}".to_owned());

        let text = super::super::write_request(&draft);
        let requests = parse_requests(&text).expect("one request");

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].name, "Round trip");
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].url, "https://example.com/things");
        assert_eq!(requests[0].body.as_deref(), Some("{\"ok\":true}"));
    }

    #[test]
    fn empty_input_is_rejected() {
        assert!(matches!(
            parse_requests("   \n\n"),
            Err(HttpFormatError::Empty)
        ));
    }

    #[test]
    fn comment_only_file_is_rejected() {
        assert!(matches!(
            parse_requests("# only a comment\n// and another\n"),
            Err(HttpFormatError::MissingRequestLine)
        ));
    }
}

//! Shell-style tokenizer for cURL command lines.
//!
//! Splits a `curl …` command into argv-style tokens the way a POSIX shell
//! would, which is what users get when they copy a command from a terminal,
//! browser dev-tools ("Copy as cURL"), or API docs. It is deliberately a small
//! subset — enough to faithfully split a curl invocation, not a full shell.
//!
//! Handled: single quotes (fully literal), double quotes (with `\"`, `\\`,
//! `\$`, `` \` `` escapes), backslash escapes outside quotes, and
//! `\`-newline / `^`-newline line continuations (the latter for commands
//! copied from Windows `cmd`). Adjacent quoted and unquoted runs concatenate
//! into a single token (`-d'{"a":1}'` → one token), matching shell word rules.

use super::CurlParseError;

#[derive(PartialEq, Eq)]
enum Mode {
    Normal,
    Single,
    Double,
}

/// Split `input` into shell-style tokens. Returns an error only for an
/// unterminated quote — everything else degrades gracefully.
pub fn tokenize(input: &str) -> Result<Vec<String>, CurlParseError> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    // Distinguishes "no token yet" from "an empty token" (e.g. `''`), so an
    // explicit empty quoted argument survives.
    let mut has_token = false;
    let mut mode = Mode::Normal;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match mode {
            Mode::Normal => match c {
                ' ' | '\t' | '\r' | '\n' => {
                    if has_token {
                        tokens.push(std::mem::take(&mut current));
                        has_token = false;
                    }
                }
                '\'' => {
                    has_token = true;
                    mode = Mode::Single;
                }
                '"' => {
                    has_token = true;
                    mode = Mode::Double;
                }
                '\\' => match chars.next() {
                    // Backslash-newline (and \r\n) is a line continuation.
                    Some('\n') => {}
                    Some('\r') => {
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                    }
                    Some(other) => {
                        current.push(other);
                        has_token = true;
                    }
                    // Trailing backslash: keep it literally.
                    None => {
                        current.push('\\');
                        has_token = true;
                    }
                },
                '^' => {
                    // Windows `cmd` caret line-continuation: only meaningful
                    // immediately before a newline; otherwise a literal caret.
                    match chars.peek() {
                        Some('\n') => {
                            chars.next();
                        }
                        Some('\r') => {
                            chars.next();
                            if chars.peek() == Some(&'\n') {
                                chars.next();
                            }
                        }
                        _ => {
                            current.push('^');
                            has_token = true;
                        }
                    }
                }
                other => {
                    current.push(other);
                    has_token = true;
                }
            },
            Mode::Single => match c {
                '\'' => mode = Mode::Normal,
                other => current.push(other),
            },
            Mode::Double => match c {
                '"' => mode = Mode::Normal,
                '\\' => match chars.next() {
                    // Inside double quotes only these are escapes; other
                    // backslashes are preserved verbatim (POSIX rule).
                    Some(n @ ('"' | '\\' | '$' | '`')) => current.push(n),
                    Some('\n') => {}
                    Some(other) => {
                        current.push('\\');
                        current.push(other);
                    }
                    None => current.push('\\'),
                },
                other => current.push(other),
            },
        }
    }

    if mode != Mode::Normal {
        return Err(CurlParseError::UnterminatedQuote);
    }
    if has_token {
        tokens.push(current);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_whitespace() {
        assert_eq!(
            tokenize("curl -X GET url").unwrap(),
            ["curl", "-X", "GET", "url"]
        );
    }

    #[test]
    fn single_quotes_are_literal() {
        assert_eq!(
            tokenize("curl -d '{\"a\": 1, \"b\": 2}'").unwrap(),
            ["curl", "-d", "{\"a\": 1, \"b\": 2}"]
        );
    }

    #[test]
    fn double_quotes_group_and_escape() {
        assert_eq!(
            tokenize("curl -H \"Auth: \\\"x\\\"\"").unwrap(),
            ["curl", "-H", "Auth: \"x\""]
        );
    }

    #[test]
    fn adjacent_runs_concatenate() {
        assert_eq!(tokenize("-d'{\"a\":1}'").unwrap(), ["-d{\"a\":1}"]);
        assert_eq!(tokenize("\"a\"b'c'").unwrap(), ["abc"]);
    }

    #[test]
    fn empty_quotes_yield_empty_token() {
        assert_eq!(tokenize("-d ''").unwrap(), ["-d", ""]);
    }

    #[test]
    fn backslash_newline_is_continuation() {
        assert_eq!(
            tokenize("curl url \\\n  -H 'A: b'").unwrap(),
            ["curl", "url", "-H", "A: b"]
        );
    }

    #[test]
    fn caret_newline_is_continuation() {
        assert_eq!(
            tokenize("curl url ^\r\n -k").unwrap(),
            ["curl", "url", "-k"]
        );
    }

    #[test]
    fn backslash_escapes_space_outside_quotes() {
        assert_eq!(tokenize(r"a\ b").unwrap(), ["a b"]);
    }

    #[test]
    fn unterminated_single_quote_errors() {
        assert!(matches!(
            tokenize("curl -d 'oops"),
            Err(CurlParseError::UnterminatedQuote)
        ));
    }

    #[test]
    fn unterminated_double_quote_errors() {
        assert!(matches!(
            tokenize("curl -H \"oops"),
            Err(CurlParseError::UnterminatedQuote)
        ));
    }
}

use anyhow::{bail, Context, Result};
use lazy_static::lazy_static;
use regex::Regex;

// Use lazy static to ensure regex is only compiled once
lazy_static! {
    // Regex for the start of a hunk. The start of a hunk should look like:
    //
    //      `@@ -731,7 +731,7 @@[...]`
    //
    static ref HUNK_START: Regex = Regex::new(r"^@@ -\d+,\d+ \+(?P<start>\d+),\d+ @@").unwrap();
}

/// Represents a single comment on a review
#[derive(Debug, PartialEq)]
pub struct ReviewComment {
    /// File the comment is in
    ///
    /// Note that this is the new filename if the file was also moved
    pub file: String,
    /// The line number on the "new" side of the diff a comment applies to
    pub line: u64,
    /// For a spanned comment, the first line of the span. See `line` for docs on semantics
    pub start_line: Option<u64>,
    /// The user-supplied review comment
    pub comment: String,
}

struct FilePreambleState {
    /// Relative path of the file under diff
    file: String,
}

#[derive(Clone)]
struct FileDiffState {
    /// Relative path of the file under diff
    file: String,
    /// Current line. See `ReviewComment::line` for docs on semantics of `line`
    line: u64,
    /// First line of the span. See `ReviewComment::line` for docs on
    /// semantics of `line`
    span_start_line: Option<u64>,
}

struct SpanStartOrCommentState {
    /// State of the file diff before we entered this state
    file_diff_state: FileDiffState,
}

struct CommentState {
    /// State of the file diff before we entered comment processing
    file_diff_state: FileDiffState,
    /// Each line of comment is stored as an entry
    comment: Vec<String>,
}

/// State machine states
///
/// Only the following state transitions are valid:
///
///                                  +---------------+
///                                  |               |
///                                  v               |
///     Start -> FilePreamble -> FileDiff -> StartSpanOrComment -> Comment
///                 ^    ^        |  | ^                            ^   |
///                 |    |        |  | |                            |   |
///                 |    +--------+--+-+----------------------------+---+
///                 |             |  |                              |
///                 +-------------+  +------------------------------+
///
enum State {
    /// Starting state
    Start,
    /// The `diff --git a/...` preamble as well as the lines before the first hunk
    FilePreamble(FilePreambleState),
    /// We are inside the diff of a file
    FileDiff(FileDiffState),
    /// We are either the start of a span or the beginning of a comment
    ///
    /// The uncertainty comes from the fact that comments typically begin with one
    /// or more newlines
    SpanStartOrComment(SpanStartOrCommentState),
    /// We are inside a user-supplied comment
    Comment(CommentState),
}

/// Simple state machine to parse a review file
pub struct ReviewParser {
    state: State,
}

fn is_diff_header(s: &str) -> bool {
    s.starts_with("diff --git ")
}

/// Parses the new filename out of a diff header
fn parse_diff_header(line: &str) -> Result<String> {
    let parts: Vec<&str> = line.split(' ').collect();
    if parts.len() != 4 {
        bail!(
            "Invalid diff header: expected 4 parts, found {}",
            parts.len()
        );
    }

    // Final part of diff header will be something like:
    //
    //      `b/path/to/file`
    //
    if !parts[3].starts_with("b/") {
        bail!("Invalid diff header: final file path does not begin with 'b/'");
    }

    Ok(parts[3][2..].trim().to_owned())
}

/// Parses the starting line out of the hunk start
fn parse_hunk_start(line: &str) -> Result<Option<u64>> {
    if let Some(captures) = HUNK_START.captures(line) {
        let hunk_start_line: u64 = captures
            .name("start")
            .unwrap()
            .as_str()
            .parse()
            .context("Failed to parse hunk start line")?;

        if hunk_start_line == 0 {
            bail!("Invalid hunk start line of 0");
        }

        return Ok(Some(hunk_start_line));
    }

    Ok(None)
}

impl ReviewParser {
    pub fn new() -> ReviewParser {
        ReviewParser {
            state: State::Start,
        }
    }

    pub fn parse_line(&mut self, mut line: &str) -> Result<Option<ReviewComment>> {
        let is_quoted = line.starts_with("> ");
        if is_quoted {
            line = &line[2..];
        }

        match &mut self.state {
            State::Start => {
                if !is_quoted {
                    bail!("Unexpected comment in start state");
                }

                if !is_diff_header(line) {
                    bail!("Expected diff header from start state, found '{}'", line);
                }

                self.state = State::FilePreamble(FilePreambleState {
                    file: parse_diff_header(line)?,
                });

                Ok(None)
            }
            State::FilePreamble(state) => {
                if !is_quoted {
                    bail!(
                        "Unexpected comment in file preamble state, file: {}",
                        state.file
                    );
                }

                if let Some(start_line) = parse_hunk_start(line)? {
                    self.state = State::FileDiff(FileDiffState {
                        file: state.file.to_owned(),
                        // Subtract 1 b/c this line is before the actual diff hunk
                        line: start_line - 1,
                        span_start_line: None,
                    });
                }

                Ok(None)
            }
            State::FileDiff(state) => {
                if is_quoted {
                    if is_diff_header(line) {
                        if state.span_start_line.is_some() {
                            bail!(
                                "Detected span that was not terminated with a comment, file: {}",
                                state.file
                            );
                        }

                        self.state = State::FilePreamble(FilePreambleState {
                            file: parse_diff_header(line)?,
                        });
                    } else if let Some(start_line) = parse_hunk_start(line)? {
                        // Subtract 1 b/c this line is before the actual diff hunk
                        state.line = start_line - 1;
                    } else {
                        state.line += 1;
                    }

                    return Ok(None);
                }

                // Now that we know this line is not quoted, there's only two options:
                // 1) beginning of a spanned comment
                // 2) beginning of a comment
                if line.trim().is_empty() {
                    self.state = State::SpanStartOrComment(SpanStartOrCommentState {
                        file_diff_state: state.clone(),
                    })
                } else {
                    self.state = State::Comment(CommentState {
                        file_diff_state: state.clone(),
                        comment: vec![line.to_owned()],
                    })
                }

                Ok(None)
            }
            State::SpanStartOrComment(state) => {
                if is_quoted {
                    if state.file_diff_state.span_start_line.is_some() {
                        bail!(
                            "Detected span that was not terminated with a comment, file: {}",
                            state.file_diff_state.file
                        );
                    }

                    // Back to the original file diff
                    let next_pos = state.file_diff_state.line + 1;
                    self.state = State::FileDiff(FileDiffState {
                        file: state.file_diff_state.file.to_owned(),
                        line: next_pos,
                        span_start_line: Some(next_pos),
                    });

                    Ok(None)
                } else if line.trim().is_empty() {
                    // In a multi-line span spart
                    Ok(None)
                } else {
                    // In a comment now
                    self.state = State::Comment(CommentState {
                        file_diff_state: state.file_diff_state.clone(),
                        comment: vec![line.to_owned()],
                    });

                    Ok(None)
                }
            }
            State::Comment(state) => {
                if is_quoted {
                    let comment = ReviewComment {
                        file: state.file_diff_state.file.clone(),
                        line: state.file_diff_state.line,
                        start_line: state.file_diff_state.span_start_line,
                        comment: state.comment.join("\n").trim_end().to_string(),
                    };

                    if is_diff_header(line) {
                        self.state = State::FilePreamble(FilePreambleState {
                            file: parse_diff_header(line)?,
                        });
                    } else {
                        self.state = State::FileDiff(FileDiffState {
                            file: state.file_diff_state.file.to_owned(),
                            line: state.file_diff_state.line + 1,
                            span_start_line: None,
                        });
                    }

                    return Ok(Some(comment));
                }

                state.comment.push(line.to_owned());
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_fail(input: &str) {
        let mut parser = ReviewParser::new();

        for line in input.lines() {
            if let Err(_) = parser.parse_line(line) {
                return;
            }
        }

        panic!("Parser succeeded when it should have failed");
    }

    fn test(input: &str, expected: &[ReviewComment]) {
        let mut parser = ReviewParser::new();
        let mut comments = Vec::new();

        for line in input.lines() {
            if let Some(c) = parser.parse_line(line).unwrap() {
                comments.push(c);
            }
        }

        assert!(
            comments == expected,
            "Parsed different comments than expected.\n Got: {:#?}\nExpected: {:#?}",
            comments,
            expected
        );
    }

    #[test]
    fn single_comment() {
        let input = include_str!("../testdata/single_comment");
        let expected = vec![ReviewComment {
            file: "libbpf-cargo/src/btf/btf.rs".to_string(),
            line: 735,
            start_line: Some(731),
            comment: "Comment 1".to_string(),
        }];

        test(input, &expected);
    }

    #[test]
    fn multiline_comment() {
        let input = include_str!("../testdata/multiline_comment");
        let expected = vec![ReviewComment {
            file: "libbpf-cargo/src/btf/btf.rs".to_string(),
            line: 737,
            start_line: None,
            comment: "Comment line 1\nComment line 2\n\nComment line 4".to_string(),
        }];

        test(input, &expected);
    }

    #[test]
    fn back_to_back_span() {
        let input = include_str!("../testdata/back_to_back_span");
        let expected = vec![
            ReviewComment {
                file: "libbpf-cargo/src/btf/btf.rs".to_string(),
                line: 735,
                start_line: Some(731),
                comment: "Comment 1".to_string(),
            },
            ReviewComment {
                file: "libbpf-cargo/src/btf/btf.rs".to_string(),
                line: 738,
                start_line: None,
                comment: "Comment 2".to_string(),
            },
        ];

        test(input, &expected);
    }

    #[test]
    fn multiple_files() {
        let input = include_str!("../testdata/multiple_files");
        let expected = vec![
            ReviewComment {
                file: "libbpf-cargo/src/btf/btf.rs".to_string(),
                line: 735,
                start_line: None,
                comment: "Comment 1".to_string(),
            },
            ReviewComment {
                file: "libbpf-cargo/src/test.rs".to_string(),
                line: 2159,
                start_line: None,
                comment: "Comment 2".to_string(),
            },
        ];

        test(input, &expected);
    }

    #[test]
    fn hunk_start_no_trailing_whitespace() {
        let input = include_str!("../testdata/hunk_start_no_trailing_whitespace");
        let expected = vec![ReviewComment {
            file: "ch5.txt".to_string(),
            line: 7,
            start_line: None,
            comment: "Great passage".to_string(),
        }];

        test(input, &expected);
    }

    #[test]
    fn unterminated_span() {
        let input = include_str!("../testdata/unterminated_span");
        test_fail(input);
    }

    #[test]
    fn cross_file_span_ignored() {
        let input = include_str!("../testdata/cross_file_span_ignored");
        test_fail(input);
    }

    #[test]
    fn unterminated_back_to_back_span() {
        let input = include_str!("../testdata/unterminated_back_to_back_span");
        test_fail(input);
    }
}

use anyhow::{bail, Result};

/// Represents a single comment on a review
#[derive(Debug, PartialEq)]
pub struct ReviewComment {
    /// File the comment is in
    ///
    /// Note that this is the new filename if the file was also moved
    file: String,
    /// The "line" a comment applies to. To quote github API:
    ///
    /// The position value equals the number of lines down from the first "@@" hunk header in the
    /// file you want to add a comment. The line just below the "@@" line is position 1, the next
    /// line is position 2, and so on. The position in the diff continues to increase through lines
    /// of whitespace and additional hunks until the beginning of a new file.
    position: u64,
    /// For a spanned comment, the first line of the span. See `position` for docs on semantics
    start_position: Option<u64>,
    /// The user-supplied review comment
    comment: String,
}

struct FilePreambleState {
    /// Relative path of the file under diff
    file: String,
}

#[derive(Clone)]
struct FileDiffState {
    /// Relative path of the file under diff
    file: String,
    /// Current position. See `ReviewComment::position` for docs on semantics of `position`
    position: u64,
    /// Position of the start of the span. See `ReviewComment::position` for docs on
    /// semantics of `position`
    span_start_position: Option<u64>,
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
                    bail!("Unexpected comment in file preamble state");
                }

                if let Some(stripped) = line.strip_prefix("@@ ") {
                    // Extra sanity check; the start of a hunk should look like:
                    //
                    //      `@@ -731,7 +731,7 @@ [...]`
                    //
                    if stripped.contains(" @@ ") {
                        self.state = State::FileDiff(FileDiffState {
                            file: state.file.to_owned(),
                            position: 0,
                            span_start_position: None,
                        });
                    }
                }

                Ok(None)
            }
            State::FileDiff(state) => {
                if is_quoted {
                    if is_diff_header(line) {
                        self.state = State::FilePreamble(FilePreambleState {
                            file: parse_diff_header(line)?,
                        });
                    } else {
                        state.position += 1;
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
                    if state.file_diff_state.span_start_position.is_some() {
                        bail!("Detected span that was not terminated with a comment");
                    }

                    // Back to the original file diff
                    let next_pos = state.file_diff_state.position + 1;
                    self.state = State::FileDiff(FileDiffState {
                        file: state.file_diff_state.file.to_owned(),
                        position: next_pos,
                        span_start_position: Some(next_pos),
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
                        position: state.file_diff_state.position,
                        start_position: state.file_diff_state.span_start_position,
                        comment: state.comment.join("\n").trim_end().to_string(),
                    };

                    if is_diff_header(line) {
                        self.state = State::FilePreamble(FilePreambleState {
                            file: parse_diff_header(line)?,
                        });
                    } else {
                        self.state = State::FileDiff(FileDiffState {
                            file: state.file_diff_state.file.to_owned(),
                            position: state.file_diff_state.position + 1,
                            span_start_position: None,
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
            position: 5,
            start_position: Some(1),
            comment: "Comment 1".to_string(),
        }];

        test(input, &expected);
    }

    #[test]
    fn multiline_comment() {
        let input = include_str!("../testdata/multiline_comment");
        let expected = vec![ReviewComment {
            file: "libbpf-cargo/src/btf/btf.rs".to_string(),
            position: 7,
            start_position: None,
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
                position: 5,
                start_position: Some(1),
                comment: "Comment 1".to_string(),
            },
            ReviewComment {
                file: "libbpf-cargo/src/btf/btf.rs".to_string(),
                position: 8,
                start_position: None,
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
                position: 5,
                start_position: None,
                comment: "Comment 1".to_string(),
            },
            ReviewComment {
                file: "libbpf-cargo/src/test.rs".to_string(),
                position: 15,
                start_position: None,
                comment: "Comment 2".to_string(),
            },
        ];

        test(input, &expected);
    }

    #[test]
    fn unterminated_span() {
        let input = include_str!("../testdata/unterminated_span");
        let expected = vec![];
        test(input, &expected);
    }

    #[test]
    fn cross_file_span_ignored() {
        let input = include_str!("../testdata/cross_file_span_ignored");
        let expected = vec![ReviewComment {
            file: "libbpf-cargo/src/test.rs".to_string(),
            position: 12,
            start_position: None,
            comment: "Comment 1".to_string(),
        }];

        test(input, &expected);
    }

    #[test]
    fn unterminated_back_to_back_span() {
        let input = include_str!("../testdata/unterminated_back_to_back_span");
        test_fail(input);
    }
}

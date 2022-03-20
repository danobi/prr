use anyhow::Result;

enum State {
    Start,
}

/// Simple state machine to parse a review file
pub struct ReviewParser {
    state: State,
}

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

impl ReviewParser {
    pub fn new() -> ReviewParser {
        ReviewParser {
            state: State::Start,
        }
    }

    pub fn parse_line(&mut self, line: &str) -> Result<Option<ReviewComment>> {
        // XXX: implement
        unimplemented!();
    }
}

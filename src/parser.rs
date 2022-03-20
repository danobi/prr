use anyhow::Result;

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

struct FileDiffState {
    /// Current position. See `ReviewComment::position` for docs on semantics of `position`
    position: u64,
    /// Position of the start of the span. See `ReviewComment::position` for docs on
    /// semantics of `position`
    span_start_position: Option<u64>,
}

struct CommentState {
    /// State of the file diff before we entered comment processing
    file_diff_state: FileDiffState,
    /// Each line of comment is stored as an entry
    comment: Vec<String>,
}

enum State {
    /// Starting state
    Start,
    /// The `diff --git a/...` preamble as well as the lines before the first hunk
    FilePreamble,
    /// We are inside the diff of a file
    FileDiff(FileDiffState),
    /// We are inside a user-supplied comment
    Comment(CommentState),
}

/// Simple state machine to parse a review file
pub struct ReviewParser {
    state: State,
}

impl ReviewParser {
    pub fn new() -> ReviewParser {
        ReviewParser {
            state: State::Start,
        }
    }

    pub fn parse_line(&mut self, _line: &str) -> Result<Option<ReviewComment>> {
        // XXX: implement
        unimplemented!();
    }
}

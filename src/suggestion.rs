//! Filters review comment bodies down to only their GitHub suggestion blocks.
//!
//! This fork only ever posts code suggestions. Any prose a reviewer writes
//! around a ```suggestion block is dropped before submission.

/// Returns only the fenced ```suggestion blocks found in `body`, joined by a
/// blank line. Returns `None` if `body` contains no suggestion block.
pub fn only_suggestions(body: &str) -> Option<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut open: Option<(usize, Vec<String>)> = None;

    for line in body.lines() {
        let trimmed = line.trim();
        let ticks = trimmed.chars().take_while(|c| *c == '`').count();

        match open.as_mut() {
            None => {
                if ticks >= 3 && trimmed[ticks..].trim().starts_with("suggestion") {
                    open = Some((ticks, vec![line.to_string()]));
                }
            }
            Some((fence_len, lines)) => {
                lines.push(line.to_string());
                // A closing fence is at least as long as the opening one and
                // carries no info string
                if ticks >= *fence_len && trimmed[ticks..].trim().is_empty() {
                    let (_, lines) = open.take().unwrap();
                    blocks.push(lines.join("\n"));
                }
            }
        }
    }

    // An unterminated block would render as garbage on GitHub, so drop it

    if blocks.is_empty() {
        None
    } else {
        Some(blocks.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_suggestion() {
        assert_eq!(only_suggestions("just some prose\nand more"), None);
        assert_eq!(only_suggestions("```rust\nlet x = 1;\n```"), None);
    }

    #[test]
    fn strips_surrounding_prose() {
        let body = "nit: rename this\n\n```suggestion\nlet count = 1;\n```\n\nthanks!";
        assert_eq!(
            only_suggestions(body).unwrap(),
            "```suggestion\nlet count = 1;\n```"
        );
    }

    #[test]
    fn keeps_multiple_blocks() {
        let body = "a\n```suggestion\nfoo\n```\nb\n```suggestion\nbar\n```\nc";
        assert_eq!(
            only_suggestions(body).unwrap(),
            "```suggestion\nfoo\n```\n\n```suggestion\nbar\n```"
        );
    }

    #[test]
    fn keeps_inner_fences_with_longer_outer_fence() {
        let body = "````suggestion\n```\nnested\n```\n````";
        assert_eq!(
            only_suggestions(body).unwrap(),
            "````suggestion\n```\nnested\n```\n````"
        );
    }

    #[test]
    fn empty_suggestion_body_is_kept() {
        // Deleting a line is a legitimate empty suggestion
        assert_eq!(
            only_suggestions("drop it\n```suggestion\n```").unwrap(),
            "```suggestion\n```"
        );
    }

    #[test]
    fn unterminated_block_dropped() {
        assert_eq!(only_suggestions("```suggestion\nlet x = 1;"), None);
    }

    #[test]
    fn indented_fence() {
        let body = "  ```suggestion\n  let x = 1;\n  ```";
        assert_eq!(only_suggestions(body).unwrap(), body);
    }
}

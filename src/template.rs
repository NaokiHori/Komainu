//! Template scanning. Only `{{ ... }}` is recognized; Markdown is not interpreted.
//!
//! A `{{` opens a placeholder only if it is directly followed by whitespace (D2, R4.3);
//! otherwise the `{` is plain text (so LaTeX such as `\epsilon_{{ijk}}` passes through).

use crate::diagnostic::Problem;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Segment<'a> {
    Text(&'a str),
    /// The trimmed content of a placeholder and its byte offset in the template.
    Placeholder {
        content: &'a str,
        offset: usize,
    },
}

pub(crate) struct Parsed<'a> {
    pub segments: Vec<Segment<'a>>,
    pub errors: Vec<Problem>,
}

fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

pub(crate) fn parse(template: &str) -> Parsed<'_> {
    let bytes = template.as_bytes();
    let mut segments = Vec::new();
    let mut errors = Vec::new();
    let mut text_start = 0;
    let mut i = 0;
    while i + 2 < bytes.len() {
        if !(bytes[i] == b'{' && bytes[i + 1] == b'{' && is_whitespace(bytes[i + 2])) {
            i += 1;
            continue;
        }
        if text_start < i {
            segments.push(Segment::Text(&template[text_start..i]));
        }
        let content_start = i + 2;
        let Some(close) = template[content_start..]
            .find("}}")
            .map(|p| content_start + p)
        else {
            errors.push(Problem::syntax(
                i,
                "unterminated placeholder: expected `}}`",
            ));
            // The rest of the template is swallowed by the broken placeholder.
            return Parsed { segments, errors };
        };
        if !is_whitespace(bytes[close - 1]) {
            errors.push(Problem::syntax(close, "expected whitespace before `}}`"));
        } else {
            let raw = &template[content_start..close];
            let is_ws = |c: char| c.is_ascii() && is_whitespace(c as u8);
            let content = raw.trim_matches(is_ws);
            if content.is_empty() {
                errors.push(Problem::syntax(i, "empty placeholder"));
            } else {
                let offset = content_start + (raw.len() - raw.trim_start_matches(is_ws).len());
                segments.push(Segment::Placeholder { content, offset });
            }
        }
        i = close + 2;
        text_start = i;
    }
    if text_start < bytes.len() {
        segments.push(Segment::Text(&template[text_start..]));
    }
    Parsed { segments, errors }
}

/// Converts a byte offset into a 1-based (line, column); the column counts characters.
pub(crate) fn line_col(template: &str, offset: usize) -> (u32, u32) {
    let before = &template[..offset];
    let line = before.matches('\n').count() as u32 + 1;
    let line_start = before.rfind('\n').map_or(0, |p| p + 1);
    let col = before[line_start..].chars().count() as u32 + 1;
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(template: &str) -> Vec<&str> {
        parse(template)
            .segments
            .into_iter()
            .filter_map(|s| match s {
                Segment::Placeholder { content, .. } => Some(content),
                Segment::Text(_) => None,
            })
            .collect()
    }

    #[test]
    fn latex_double_braces_are_text() {
        let parsed = parse(r"\epsilon_{{ijk}} {{n}}");
        assert!(parsed.errors.is_empty());
        assert_eq!(
            parsed.segments,
            vec![Segment::Text(r"\epsilon_{{ijk}} {{n}}")]
        );
    }

    #[test]
    fn triple_braces_open_at_the_last_two() {
        let parsed = parse("{{{ a }}}");
        assert!(parsed.errors.is_empty());
        assert_eq!(
            parsed.segments,
            vec![
                Segment::Text("{"),
                Segment::Placeholder {
                    content: "a",
                    offset: 4
                },
                Segment::Text("}"),
            ]
        );
    }

    #[test]
    fn multiline_placeholder() {
        assert_eq!(placeholders("{{\n  a\n}}"), vec!["a"]);
    }

    #[test]
    fn unterminated_and_missing_space_are_errors() {
        assert_eq!(parse("x {{ a").errors.len(), 1);
        let e = parse("{{ a}}").errors;
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].offset, 4);
    }

    #[test]
    fn empty_placeholder_is_an_error() {
        assert_eq!(parse("{{ }}").errors.len(), 1);
    }

    #[test]
    fn line_and_column() {
        assert_eq!(line_col("ab\nあc", 0), (1, 1));
        assert_eq!(line_col("ab\nあc", 6), (2, 2));
    }
}

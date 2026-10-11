//! Toggle `//` line comments over a range of lines (pure, tested without a window).

use std::ops::Range;

/// Byte range covering the full lines touched by `selection`.
pub fn line_span(text: &str, selection: &Range<usize>) -> Range<usize> {
    let start = text[..selection.start].rfind('\n').map_or(0, |i| i + 1);
    // A selection ending at the start of a line does not include that line.
    let end_anchor = if selection.end > selection.start && text[..selection.end].ends_with('\n') {
        selection.end - 1
    } else {
        selection.end
    };
    let end = text[end_anchor..]
        .find('\n')
        .map_or(text.len(), |i| end_anchor + i);
    start..end
}

/// Comment the lines if any non-blank line is uncommented, otherwise uncomment them.
pub fn toggle(lines: &str) -> String {
    let content: Vec<&str> = lines.split('\n').collect();
    let all_commented = content
        .iter()
        .filter(|l| !l.trim().is_empty())
        .all(|l| l.trim_start().starts_with("//"));
    let indent = content
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    content
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                (*line).to_owned()
            } else if all_commented {
                let lead = line.len() - line.trim_start().len();
                let rest = &line[lead + 2..];
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                format!("{}{rest}", &line[..lead])
            } else {
                format!("{}// {}", &line[..indent], &line[indent..])
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_uncomments() {
        let text = "g.V().\n  out()\n\ng.E()";
        assert_eq!(toggle("g.V().\n  out()"), "// g.V().\n//   out()");
        assert_eq!(toggle("// g.V().\n//   out()"), "g.V().\n  out()");
        assert_eq!(toggle("  a\n  // b"), "  // a\n  // // b");
        assert_eq!(line_span(text, &(9..9)), 7..14);
        assert_eq!(line_span(text, &(0..8)), 0..14);
        assert_eq!(
            line_span(text, &(0..7)),
            0..6,
            "ending at a line start excludes that line"
        );
    }
}

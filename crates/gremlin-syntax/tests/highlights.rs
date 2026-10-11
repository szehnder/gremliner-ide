//! Snapshot of highlight captures for a representative query.

#![allow(clippy::unwrap_used)]

use gremlin_syntax::{Document, HIGHLIGHTS_QUERY, language};
use tree_sitter::{Query, QueryCursor, StreamingIterator as _};

#[test]
fn highlight_captures() {
    let text = "// top airports\ng.V().hasLabel('airport').has('runways', P.gt(3)).order().by('elev', desc).limit(5L).valueMap(true)";
    let doc = Document::parse(text);
    let query = Query::new(&language(), HIGHLIGHTS_QUERY).unwrap();
    let mut cursor = QueryCursor::new();
    let mut captures = cursor.captures(&query, doc.tree().root_node(), text.as_bytes());
    let mut lines = Vec::new();
    while let Some((m, index)) = captures.next() {
        let capture = m.captures[*index];
        let name = query.capture_names()[capture.index as usize];
        if name.starts_with("punctuation") {
            continue;
        }
        lines.push(format!("{:<20} {}", name, &text[capture.node.byte_range()]));
    }
    insta::assert_snapshot!(lines.join("\n"));
}

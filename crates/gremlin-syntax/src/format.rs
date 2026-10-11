//! The formatter, following Gremlint conventions: a statement that fits stays on one line with normalised
//! spacing; otherwise each step goes on its own line with a trailing `.` (valid in the Groovy console),
//! continuation lines are indented by 2, and arguments that do not fit are broken one per line.
//!
//! Statements containing comments or syntax errors are left exactly as written.

use tree_sitter::Node;

use crate::document::Document;

pub const DEFAULT_WIDTH: usize = 80;
const INDENT: usize = 2;

/// Format a whole document.
pub fn format(text: &str, width: usize) -> String {
    let doc = Document::parse(text);
    let root = doc.tree().root_node();
    let mut out = String::new();
    let mut prev_end: Option<usize> = None;
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if let Some(end) = prev_end {
            let gap = &text[end..child.start_byte()];
            let newlines = gap.matches('\n').count();
            let is_comment = matches!(child.kind(), "line_comment" | "block_comment");
            out.push_str(match newlines {
                0 if is_comment => " ",
                0 | 1 => "\n",
                _ => "\n\n",
            });
        }
        let verbatim = matches!(child.kind(), "line_comment" | "block_comment")
            || child.has_error()
            || contains_comment(child)
            || text[child.byte_range()].contains('\n') && contains_kind(child, "lambda");
        if verbatim {
            out.push_str(text[child.byte_range()].trim_end());
        } else {
            out.push_str(&Fmt { text, width }.node(child, 0, 0));
        }
        prev_end = Some(child.end_byte());
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn contains_comment(node: Node<'_>) -> bool {
    contains_kind(node, "line_comment") || contains_kind(node, "block_comment")
}

fn contains_kind(node: Node<'_>, kind: &str) -> bool {
    if node.kind() == kind {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|c| contains_kind(c, kind))
}

struct Fmt<'a> {
    text: &'a str,
    width: usize,
}

impl Fmt<'_> {
    fn src(&self, node: Node<'_>) -> &str {
        &self.text[node.byte_range()]
    }

    fn named<'t>(&self, node: Node<'t>) -> Vec<Node<'t>> {
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment"))
            .collect()
    }

    /// The node on one line with normalised spacing.
    fn flat(&self, node: Node<'_>) -> String {
        match node.kind() {
            "traversal" => {
                let parts = self.named(node);
                let mut s = String::new();
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        s.push('.');
                    }
                    s.push_str(&self.flat(*part));
                }
                s
            }
            "call" => {
                let mut s = self
                    .src(node.child_by_field_name("name").unwrap_or(node))
                    .to_owned();
                if let Some(args) = node.child_by_field_name("arguments") {
                    s.push_str(&self.flat(args));
                }
                if let Some(closure) = node.child_by_field_name("closure") {
                    s.push_str(self.src(closure));
                }
                s
            }
            "arguments" => format!("({})", self.join_flat(node)),
            "list" => format!("[{}]", self.join_flat(node)),
            "map" => {
                let pairs = self.named(node);
                if pairs.is_empty() {
                    "[:]".to_owned()
                } else {
                    format!("[{}]", self.join_flat(node))
                }
            }
            "pair" => {
                let key = node
                    .child_by_field_name("key")
                    .map(|k| self.flat(k))
                    .unwrap_or_default();
                let value = node
                    .child_by_field_name("value")
                    .map(|v| self.flat(v))
                    .unwrap_or_default();
                format!("{key}: {value}")
            }
            "parenthesized" => {
                let inner = self
                    .named(node)
                    .first()
                    .map(|n| self.flat(*n))
                    .unwrap_or_default();
                format!("({inner})")
            }
            "assignment" => self.assignment(node, |f, value| f.flat(value)),
            _ => self.src(node).to_owned(),
        }
    }

    fn join_flat(&self, node: Node<'_>) -> String {
        self.named(node)
            .iter()
            .map(|n| self.flat(*n))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn assignment(&self, node: Node<'_>, value: impl Fn(&Self, Node<'_>) -> String) -> String {
        let def = if self.src(node).trim_start().starts_with("def ") {
            "def "
        } else {
            ""
        };
        let name = node
            .child_by_field_name("name")
            .map(|n| self.src(n))
            .unwrap_or("");
        let rhs = node
            .child_by_field_name("value")
            .map(|v| value(self, v))
            .unwrap_or_default();
        format!("{def}{name} = {rhs}")
    }

    /// Format `node` starting at column `col`, with continuation lines at `indent`.
    fn node(&self, node: Node<'_>, indent: usize, col: usize) -> String {
        let flat = self.flat(node);
        if col + flat.chars().count() <= self.width && !flat.contains('\n') {
            return flat;
        }
        match node.kind() {
            "traversal" => self.traversal(node, indent, col),
            "call" => self.call(node, indent, col),
            "list" | "map" => self.items(node, indent),
            "assignment" => {
                let head = self.assignment(node, |_, _| String::new());
                self.assignment(node, |f, v| f.node(v, indent, col + head.chars().count()))
            }
            _ => flat,
        }
    }

    fn traversal(&self, node: Node<'_>, indent: usize, col: usize) -> String {
        let parts = self.named(node);
        let Some((root, segments)) = parts.split_first() else {
            return String::new();
        };
        let continuation = indent + INDENT;
        let mut out = self.node(*root, indent, col);
        let mut rest = segments;
        // `g.V()` / `__.out()` stay together on the first line.
        if root.kind() == "identifier"
            && let Some((first, tail)) = segments.split_first()
        {
            out.push('.');
            let at = col + out.chars().count();
            out.push_str(&self.node(*first, continuation, at));
            rest = tail;
        }
        for segment in rest {
            out.push_str(".\n");
            out.push_str(&" ".repeat(continuation));
            out.push_str(&self.node(*segment, continuation, continuation));
        }
        out
    }

    fn call(&self, node: Node<'_>, indent: usize, col: usize) -> String {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.src(n))
            .unwrap_or("");
        let mut out = name.to_owned();
        if let Some(args) = node.child_by_field_name("arguments") {
            let items = self.named(args);
            let flat = self.flat(args);
            if items.is_empty() || col + name.len() + flat.chars().count() <= self.width {
                out.push_str(&flat);
            } else {
                let inner = indent + INDENT;
                out.push_str("(\n");
                let rendered: Vec<String> = items
                    .iter()
                    .map(|a| format!("{}{}", " ".repeat(inner), self.node(*a, inner, inner)))
                    .collect();
                out.push_str(&rendered.join(",\n"));
                out.push(')');
            }
        }
        if let Some(closure) = node.child_by_field_name("closure") {
            out.push_str(self.src(closure));
        }
        out
    }

    /// Lists and maps that do not fit: one item per line.
    fn items(&self, node: Node<'_>, indent: usize) -> String {
        let inner = indent + INDENT;
        let rendered: Vec<String> = self
            .named(node)
            .iter()
            .map(|n| format!("{}{}", " ".repeat(inner), self.node(*n, inner, inner)))
            .collect();
        format!("[\n{}]", rendered.join(",\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn structure(text: &str) -> Vec<String> {
        let doc = Document::parse(text);
        doc.statements()
            .iter()
            .flat_map(|s| doc.steps(s))
            .flat_map(|s| {
                s.walk()
                    .into_iter()
                    .map(|n| format!("{}:{}", n.depth, n.name))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn scenario_gremlin_language_format_long_query() {
        let input = "g.V().hasLabel('airport').has('country','US').out('route').has('region','US-TX').values('code').dedup().order().limit(10)";
        assert!(input.len() > 80);
        let out = format(input, 80);
        insta::assert_snapshot!(out);
        assert!(out.lines().all(|l| l.chars().count() <= 80), "{out}");
        assert!(out.lines().count() > 1);
        assert_eq!(structure(&out), structure(input), "same step structure");
        assert!(!Document::parse(&out).tree().root_node().has_error());
    }

    #[test]
    fn scenario_gremlin_language_idempotent_formatting() {
        let input = "g.V().has('person','name','marko').where(__.out('created').has('lang','java').in('created').has('age',P.gt(30))).project('name','friends').by('name').by(out('knows').values('name').fold())";
        let once = format(input, 80);
        let twice = format(&once, 80);
        assert_eq!(once, twice);
        insta::assert_snapshot!(once);
        assert_eq!(structure(&once), structure(input));
    }

    #[test]
    fn short_queries_stay_on_one_line_with_normalised_spacing() {
        assert_eq!(format("g . V( ) .has( 'a' ,1 )", 80), "g.V().has('a', 1)\n");
        assert_eq!(
            format("g.inject([a:1,'b' : [1,2]], [:])", 80),
            "g.inject([a: 1, 'b': [1, 2]], [:])\n"
        );
    }

    #[test]
    fn comments_and_statement_spacing_are_preserved() {
        let input = "// count\ng.V().count() // all\n\n\n\ng.E().count()\nx = 1";
        assert_eq!(
            format(input, 80),
            "// count\ng.V().count() // all\n\ng.E().count()\nx = 1\n"
        );
        let with_inner = "g.V().\n  // why\n  out()";
        assert_eq!(format(with_inner, 80), format!("{with_inner}\n"));
    }
}

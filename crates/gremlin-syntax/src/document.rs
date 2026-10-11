//! A parsed Gremlin document with incremental re-parsing, and the analyses built on its syntax tree:
//! statements, step trees, diagnostics, mutation and parameter detection, and step prefixes.

use std::collections::HashSet;
use std::ops::Range;

use tree_sitter::{InputEdit, Node, Parser, Point, Tree};

use crate::catalogue::Catalogue;
use crate::language::language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Byte range in the document (empty for a missing token).
    pub range: Range<usize>,
    pub message: String,
}

/// One top-level statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// Position among the root's named, non-comment children.
    pub index: usize,
    pub range: Range<usize>,
    pub text: String,
    pub has_error: bool,
}

/// A step call in a statement, with the steps of anonymous traversals in its arguments as children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepNode {
    /// Pre-order position within the statement, used to address the step (e.g. for prefixes).
    pub id: usize,
    pub name: String,
    /// The whole call, `name(args)`.
    pub span: Range<usize>,
    pub name_span: Range<usize>,
    /// 0 for the statement's main chain.
    pub depth: usize,
    pub children: Vec<StepNode>,
    /// The chain's root identifier (`g`, `__`, a variable), or empty for an anonymous chain (`out()...`).
    pub chain_root: String,
}

impl StepNode {
    /// True for steps of a Gremlin traversal (rooted at `g`, `__`, or anonymous), as opposed to Groovy
    /// method calls on variables such as `graph.features()`.
    pub fn is_gremlin(&self) -> bool {
        matches!(self.chain_root.as_str(), "" | "g" | "__")
    }
}

impl StepNode {
    /// This step and every nested step, in pre-order.
    pub fn walk(&self) -> Vec<&StepNode> {
        let mut out = vec![self];
        for child in &self.children {
            out.extend(child.walk());
        }
        out
    }
}

/// Everything known about a statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementAnalysis {
    pub statement: Statement,
    pub steps: Vec<StepNode>,
    /// True when the statement writes or deletes data, contains an opaque closure, or does not parse.
    pub is_mutating: bool,
    pub parameters: Vec<String>,
}

pub struct Document {
    text: String,
    tree: Tree,
    parser: Parser,
}

fn new_parser() -> Parser {
    let mut parser = Parser::new();
    #[allow(clippy::expect_used)]
    parser
        .set_language(&language())
        .expect("the bundled grammar matches the tree-sitter runtime");
    parser
}

impl Document {
    pub fn parse(text: impl Into<String>) -> Self {
        let text = text.into();
        let mut parser = new_parser();
        #[allow(clippy::expect_used)]
        let tree = parser
            .parse(&text, None)
            .expect("parsing without a timeout always yields a tree");
        Self { text, tree, parser }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Replace `range` (bytes) with `new_text` and re-parse incrementally.
    pub fn edit(&mut self, range: Range<usize>, new_text: &str) {
        let start_position = point_at(&self.text, range.start);
        let old_end_position = point_at(&self.text, range.end);
        self.text.replace_range(range.clone(), new_text);
        let new_end_byte = range.start + new_text.len();
        let edit = InputEdit {
            start_byte: range.start,
            old_end_byte: range.end,
            new_end_byte,
            start_position,
            old_end_position,
            new_end_position: point_at(&self.text, new_end_byte),
        };
        self.tree.edit(&edit);
        #[allow(clippy::expect_used)]
        let tree = self
            .parser
            .parse(&self.text, Some(&self.tree))
            .expect("parsing without a timeout always yields a tree");
        self.tree = tree;
    }

    fn statement_nodes(&self) -> Vec<Node<'_>> {
        let root = self.tree.root_node();
        let mut cursor = root.walk();
        root.named_children(&mut cursor)
            .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment"))
            .collect()
    }

    pub fn statements(&self) -> Vec<Statement> {
        self.statement_nodes()
            .into_iter()
            .enumerate()
            .map(|(index, node)| Statement {
                index,
                range: node.byte_range(),
                text: self.text[node.byte_range()].to_owned(),
                has_error: node.has_error() || node.is_error(),
            })
            .collect()
    }

    /// The statement containing `offset` (a cursor between statements belongs to the one it touches).
    pub fn statement_at(&self, offset: usize) -> Option<Statement> {
        self.statements()
            .into_iter()
            .find(|s| s.range.start <= offset && offset <= s.range.end)
    }

    fn node_of(&self, statement: &Statement) -> Option<Node<'_>> {
        self.statement_nodes().into_iter().nth(statement.index)
    }

    /// Syntax errors everywhere, plus warnings for unknown step names.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        collect_syntax_errors(self.tree.root_node(), &self.text, &mut out);
        for statement in self.statements() {
            for step in self.steps(&statement).iter().flat_map(StepNode::walk) {
                if step.is_gremlin() && !Catalogue::get().is_known_name(&step.name) {
                    out.push(Diagnostic {
                        severity: Severity::Warning,
                        range: step.name_span.clone(),
                        message: format!("Unknown step `{}`", step.name),
                    });
                }
            }
        }
        out.sort_by_key(|d| (d.range.start, d.range.end));
        out
    }

    /// The step tree of a statement.
    pub fn steps(&self, statement: &Statement) -> Vec<StepNode> {
        let Some(node) = self.node_of(statement) else {
            return Vec::new();
        };
        let mut next_id = 0;
        let mut out = Vec::new();
        for traversal in top_traversals(node) {
            out.extend(chain_steps(traversal, &self.text, 0, &mut next_id));
        }
        out
    }

    pub fn analyze(&self, statement: &Statement) -> StatementAnalysis {
        let steps = self.steps(statement);
        let catalogue = Catalogue::get();
        let node = self.node_of(statement);
        let has_lambda = node.is_some_and(|n| contains_kind(n, "lambda"));
        let mutating_step = steps
            .iter()
            .flat_map(StepNode::walk)
            .any(|s| catalogue.step(&s.name).is_some_and(|i| i.mutating));
        StatementAnalysis {
            is_mutating: statement.has_error || has_lambda || mutating_step,
            parameters: node.map(|n| self.parameters_in(n)).unwrap_or_default(),
            steps,
            statement: statement.clone(),
        }
    }

    /// Every statement analysed.
    pub fn analyze_all(&self) -> Vec<StatementAnalysis> {
        self.statements().iter().map(|s| self.analyze(s)).collect()
    }

    /// Names assigned anywhere in the document (`x = ...`), which are therefore not parameters.
    fn assigned_names(&self) -> HashSet<String> {
        self.statement_nodes()
            .into_iter()
            .filter(|n| n.kind() == "assignment")
            .filter_map(|n| n.child_by_field_name("name"))
            .map(|n| self.text[n.byte_range()].to_owned())
            .collect()
    }

    fn parameters_in(&self, node: Node<'_>) -> Vec<String> {
        let assigned = self.assigned_names();
        let catalogue = Catalogue::get();
        let mut out: Vec<String> = Vec::new();
        visit(node, &mut |n| {
            if n.kind() != "identifier" || !is_argument_value(n) {
                return;
            }
            let name = &self.text[n.byte_range()];
            if catalogue.is_known_name(name)
                || assigned.contains(name)
                || out.iter().any(|p| p == name)
            {
                return;
            }
            out.push(name.to_owned());
        });
        out
    }

    /// An executable query that runs the statement up to and including step `step_id` (see
    /// [`StepNode::id`]). A nested step is placed in its parent's context: the steps before the parent,
    /// then the anonymous traversal up to the step.
    pub fn prefix_at(&self, statement: &Statement, step_id: usize) -> Option<String> {
        let node = self.node_of(statement)?;
        let traversals = top_traversals(node);
        let steps = self.steps(statement);
        // Each top-level traversal is a chain; locate the chain and the step's ancestor path.
        for (traversal, chain) in traversals.iter().zip(split_chains(&steps, &traversals)) {
            if let Some(prefix) = prefix_in_chain(*traversal, &chain, step_id, &self.text, None) {
                return Some(prefix);
            }
        }
        None
    }
}

/// Group the flat top-level step list back into one list per top-level traversal.
fn split_chains(steps: &[StepNode], traversals: &[Node<'_>]) -> Vec<Vec<StepNode>> {
    traversals
        .iter()
        .map(|t| {
            let range = t.byte_range();
            steps
                .iter()
                .filter(|s| s.span.start >= range.start && s.span.end <= range.end)
                .cloned()
                .collect()
        })
        .collect()
}

/// `context` is the prefix before this chain's first step (for nested chains) or `None` for a top-level
/// chain, whose root (`g`) is taken from the text.
fn prefix_in_chain(
    traversal: Node<'_>,
    chain: &[StepNode],
    step_id: usize,
    text: &str,
    context: Option<String>,
) -> Option<String> {
    let mut before = context.unwrap_or_else(|| {
        traversal
            .child_by_field_name("root")
            .filter(|r| r.kind() == "identifier")
            .map(|r| text[r.byte_range()].to_owned())
            .unwrap_or_default()
    });
    for step in chain {
        let call_text = &text[step.span.clone()];
        let here = join_steps(&before, call_text);
        if step.id == step_id {
            return Some(here);
        }
        // Nested anonymous traversals in this step's arguments start from what came before this step.
        for (nested, nested_chain) in nested_chains(traversal, step, text) {
            if let Some(found) =
                prefix_in_chain(nested, &nested_chain, step_id, text, Some(before.clone()))
            {
                return Some(found);
            }
        }
        before = here;
    }
    None
}

/// `g` + `V()` → `g.V()`; `` + `out()` → `out()`.
fn join_steps(before: &str, call: &str) -> String {
    match before.trim_end_matches('.') {
        "" | "__" => call.to_owned(),
        b => format!("{b}.{call}"),
    }
}

/// The anonymous traversals in a step's arguments, each with its own steps.
fn nested_chains<'t>(
    traversal: Node<'t>,
    step: &StepNode,
    _text: &str,
) -> Vec<(Node<'t>, Vec<StepNode>)> {
    let Some(call) = traversal.descendant_for_byte_range(step.span.start, step.span.end) else {
        return Vec::new();
    };
    let call = if call.kind() == "call" {
        Some(call)
    } else {
        ancestor_of_kind(call, "call")
    };
    let Some(args) = call.and_then(|c| c.child_by_field_name("arguments")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for t in top_traversals(args) {
        let range = t.byte_range();
        let chain: Vec<StepNode> = step
            .children
            .iter()
            .filter(|c| c.span.start >= range.start && c.span.end <= range.end)
            .cloned()
            .collect();
        if !chain.is_empty() {
            out.push((t, chain));
        }
    }
    out
}

fn ancestor_of_kind<'t>(mut node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    while let Some(parent) = node.parent() {
        if parent.kind() == kind {
            return Some(parent);
        }
        node = parent;
    }
    None
}

/// Outermost `traversal` nodes under `node` (including `node` itself).
fn top_traversals(node: Node<'_>) -> Vec<Node<'_>> {
    if node.kind() == "traversal" {
        return vec![node];
    }
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "lambda" {
            continue;
        }
        out.extend(top_traversals(child));
    }
    out
}

/// The steps of one traversal chain, with nested anonymous traversals as children.
fn chain_steps(
    traversal: Node<'_>,
    text: &str,
    depth: usize,
    next_id: &mut usize,
) -> Vec<StepNode> {
    let catalogue = Catalogue::get();
    let root = traversal.child_by_field_name("root");
    match root {
        // `P.gt(30)`, `T.id`, `Order.desc`: tokens, not steps.
        Some(r) if r.kind() == "identifier" && catalogue.is_token_class(&text[r.byte_range()]) => {
            return Vec::new();
        }
        // A bare predicate such as `gt(30)` (statically imported).
        Some(r) if r.kind() == "call" => {
            let name = r
                .child_by_field_name("name")
                .map(|n| &text[n.byte_range()])
                .unwrap_or("");
            if !catalogue.is_step(name) && catalogue.is_predicate(name) {
                return Vec::new();
            }
        }
        _ => {}
    }
    let chain_root = root
        .filter(|r| r.kind() == "identifier")
        .map(|r| text[r.byte_range()].to_owned())
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut cursor = traversal.walk();
    for child in traversal.named_children(&mut cursor) {
        if child.kind() != "call" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        let id = *next_id;
        *next_id += 1;
        let mut children = Vec::new();
        if let Some(args) = child.child_by_field_name("arguments") {
            for nested in top_traversals(args) {
                children.extend(chain_steps(nested, text, depth + 1, next_id));
            }
        }
        out.push(StepNode {
            id,
            name: text[name_node.byte_range()].to_owned(),
            span: child.byte_range(),
            name_span: name_node.byte_range(),
            depth,
            children,
            chain_root: chain_root.clone(),
        });
    }
    out
}

fn contains_kind(node: Node<'_>, kind: &str) -> bool {
    let mut found = false;
    visit(node, &mut |n| found |= n.kind() == kind);
    found
}

fn visit<'t>(node: Node<'t>, f: &mut impl FnMut(Node<'t>)) {
    f(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit(child, f);
    }
}

/// An identifier used as a value: a call argument, list item, map value, or parenthesised value.
fn is_argument_value(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        "arguments" | "list" | "parenthesized" => true,
        "pair" => parent.child_by_field_name("value") == Some(node),
        _ => false,
    }
}

fn collect_syntax_errors(node: Node<'_>, text: &str, out: &mut Vec<Diagnostic>) {
    if node.is_missing() {
        let what = node.kind();
        let message = match what {
            ")" => "Missing closing parenthesis `)`".to_owned(),
            "]" => "Missing closing bracket `]`".to_owned(),
            "}" => "Missing closing brace `}`".to_owned(),
            "'" | "\"" => format!("Unclosed string: missing `{what}`"),
            "identifier" => "Expected a step name".to_owned(),
            other => format!("Missing `{other}`"),
        };
        out.push(Diagnostic {
            severity: Severity::Error,
            range: node.start_byte()..node.start_byte(),
            message,
        });
        return;
    }
    if node.is_error() {
        let snippet: String = text[node.byte_range()].chars().take(24).collect();
        let snippet = snippet.lines().next().unwrap_or("").trim();
        let unclosed = text[node.byte_range()].contains('(')
            && text[node.byte_range()].matches('(').count()
                > text[node.byte_range()].matches(')').count();
        let message = if unclosed && node.end_byte() == text.trim_end().len() {
            "Missing closing parenthesis `)`".to_owned()
        } else if snippet.is_empty() {
            "Unexpected input".to_owned()
        } else {
            format!("Unexpected `{snippet}`")
        };
        let range = if message.starts_with("Missing") {
            text.trim_end().len()..text.trim_end().len()
        } else {
            node.byte_range()
        };
        out.push(Diagnostic {
            severity: Severity::Error,
            range,
            message,
        });
        return;
    }
    if !node.has_error() {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_syntax_errors(child, text, out);
    }
}

/// Byte offset → tree-sitter (row, byte column).
pub fn point_at(text: &str, offset: usize) -> Point {
    let before = &text[..offset];
    let row = before.matches('\n').count();
    let column = before.rfind('\n').map_or(offset, |nl| offset - nl - 1);
    Point { row, column }
}

/// Byte offset → 1-based (line, column in characters), for messages.
pub fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, col)
}

//! A debug session over one statement: the step tree flattened into chains, the current step, and the
//! navigation rules (over, into, out, back, jump).

use std::ops::Range;

use gremlin_syntax::{Document, StepNode};

/// Steps whose arguments are evaluated as part of the step itself, not as traversals of their own.
const MODULATORS: [&str; 10] = [
    "by", "as", "option", "from", "to", "times", "until", "emit", "with", "read",
];
/// Steps whose internals cannot be shown on their own (their nested traversals are per-key reducers).
const AGGREGATES: [&str; 4] = ["project", "group", "groupCount", "math"];

/// One step of the statement, in pre-order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugStep {
    pub name: String,
    /// The whole call in the statement text, `name(args)`.
    pub span: Range<usize>,
    pub name_span: Range<usize>,
    pub depth: usize,
    /// Index of the step whose arguments contain this one.
    pub parent: Option<usize>,
    /// Index into [`Session::chains`].
    pub chain: usize,
    /// A modulator (`by`, `as`, `times`...), or a step inside one or inside `project`/`group`: results
    /// are shown at the nearest enclosing step instead.
    pub folded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError {
    Empty,
    /// More than one statement, or not a `g.` traversal.
    NotATraversal,
    Syntax,
    Mutating,
}

impl StartError {
    pub fn message(&self) -> &'static str {
        match self {
            StartError::Empty => "Put the cursor on a traversal to debug it.",
            StartError::NotATraversal => {
                "Only a single traversal starting with g. can be debugged."
            }
            StartError::Syntax => {
                "Fix the syntax errors first; the debugger needs a statement that parses."
            }
            StartError::Mutating => {
                "Queries that change data cannot be debugged: each step would run the change again."
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    text: String,
    steps: Vec<DebugStep>,
    /// Step indices of each chain, in order; chain 0 is the main traversal.
    chains: Vec<Vec<usize>>,
    current: usize,
    /// Why the current step is not the one asked for (modulators, aggregates).
    note: Option<String>,
}

impl Session {
    /// Start on `statement` (one traversal). The first step is current.
    pub fn start(statement: &str) -> Result<Session, StartError> {
        let text = statement.trim_end().trim_end_matches(';').to_owned();
        if text.trim().is_empty() {
            return Err(StartError::Empty);
        }
        let doc = Document::parse(text.as_str());
        if doc
            .diagnostics()
            .iter()
            .any(|d| d.severity == gremlin_syntax::Severity::Error)
        {
            return Err(StartError::Syntax);
        }
        let statements = doc.statements();
        if statements.len() != 1 {
            return Err(StartError::NotATraversal);
        }
        let statement = &statements[0];
        if statement.has_error {
            return Err(StartError::Syntax);
        }
        if gremlin_syntax::is_mutating(&text) {
            return Err(StartError::Mutating);
        }
        let tree = doc.steps(statement);
        if tree.is_empty() || tree.iter().any(|s| s.depth == 0 && s.chain_root != "g") {
            return Err(StartError::NotATraversal);
        }
        let mut session = Session {
            text,
            steps: Vec::new(),
            chains: vec![Vec::new()],
            current: 0,
            note: None,
        };
        for node in &tree {
            session.add(node, None, 0, false);
        }
        Ok(session)
    }

    fn add(&mut self, node: &StepNode, parent: Option<usize>, chain: usize, folded: bool) {
        let index = self.steps.len();
        let modulator = MODULATORS.contains(&node.name.as_str());
        let folded = folded || modulator;
        self.steps.push(DebugStep {
            name: node.name.clone(),
            span: node.span.clone(),
            name_span: node.name_span.clone(),
            depth: node.depth,
            parent,
            chain,
            folded,
        });
        self.chains[chain].push(index);
        let children_folded = folded || AGGREGATES.contains(&node.name.as_str());
        // Children are listed chain after chain; a comma between two of them starts a new chain.
        let mut current_chain: Option<usize> = None;
        let mut last_end: Option<usize> = None;
        for child in &node.children {
            let same = match (current_chain, last_end) {
                (Some(_), Some(end)) => {
                    let between = self.text.get(end..child.span.start).unwrap_or(",");
                    !between.contains(',') && !between.contains('(')
                }
                _ => false,
            };
            let c = match (same, current_chain) {
                (true, Some(c)) => c,
                _ => {
                    self.chains.push(Vec::new());
                    self.chains.len() - 1
                }
            };
            current_chain = Some(c);
            // Only direct children of this chain move `last_end`; their own children recurse.
            self.add(child, Some(index), c, children_folded);
            last_end = Some(child.span.end);
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn steps(&self) -> &[DebugStep] {
        &self.steps
    }

    pub fn step(&self, i: usize) -> &DebugStep {
        &self.steps[i]
    }

    pub fn chains(&self) -> &[Vec<usize>] {
        &self.chains
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// The call text of step `i`.
    pub fn call_text(&self, i: usize) -> &str {
        &self.text[self.steps[i].span.clone()]
    }

    fn position(&self, i: usize) -> (usize, usize) {
        let chain = self.steps[i].chain;
        let pos = self.chains[chain].iter().position(|&s| s == i).unwrap_or(0);
        (chain, pos)
    }

    /// The next navigable step after `i` in its chain.
    fn next_in_chain(&self, i: usize) -> Option<usize> {
        let (chain, pos) = self.position(i);
        self.chains[chain][pos + 1..]
            .iter()
            .copied()
            .find(|&s| !self.steps[s].folded)
    }

    fn prev_in_chain(&self, i: usize) -> Option<usize> {
        let (chain, pos) = self.position(i);
        self.chains[chain][..pos]
            .iter()
            .rev()
            .copied()
            .find(|&s| !self.steps[s].folded)
    }

    /// The first navigable step of the first nested traversal of `i`.
    fn first_child(&self, i: usize) -> Option<usize> {
        self.chains
            .iter()
            .filter(|c| c.first().is_some_and(|&s| self.steps[s].parent == Some(i)))
            .find_map(|c| c.iter().copied().find(|&s| !self.steps[s].folded))
    }

    fn set(&mut self, i: usize) {
        self.current = i;
        self.note = None;
    }

    /// F10: the next step in the same traversal; at the end of a nested traversal, its parent.
    pub fn step_over(&mut self) {
        if let Some(next) = self.next_in_chain(self.current) {
            self.set(next);
        } else if let Some(parent) = self.steps[self.current].parent {
            self.set(parent);
            // Leaving the nested traversal completes the parent; continue after it.
            if let Some(next) = self.next_in_chain(parent) {
                self.set(next);
            }
        }
    }

    /// F11: into the first nested traversal of the current step, else like Step Over.
    pub fn step_into(&mut self) {
        match self.first_child(self.current) {
            Some(child) => self.set(child),
            None => self.step_over(),
        }
    }

    /// Shift+F11: back to the step whose traversal contains the current one.
    pub fn step_out(&mut self) {
        if let Some(parent) = self.steps[self.current].parent {
            self.set(self.navigable(parent));
        }
    }

    /// F9: the previous step in the same traversal; at the start of a nested traversal, its parent.
    pub fn step_back(&mut self) {
        if let Some(prev) = self.prev_in_chain(self.current) {
            self.set(prev);
        } else if let Some(parent) = self.steps[self.current].parent {
            self.set(self.navigable(parent));
        }
    }

    /// The nearest step that can be shown on its own: a modulator's owner is the step it modulates
    /// (the closest earlier non-modulator in its traversal); a step inside a modulator or an aggregate
    /// belongs to its parent.
    fn navigable(&self, mut i: usize) -> usize {
        while self.steps[i].folded {
            let next = if MODULATORS.contains(&self.steps[i].name.as_str()) {
                let (chain, pos) = self.position(i);
                self.chains[chain][..pos]
                    .iter()
                    .rev()
                    .copied()
                    .find(|&s| !MODULATORS.contains(&self.steps[s].name.as_str()))
                    .or(self.steps[i].parent)
            } else {
                self.steps[i].parent
            };
            match next {
                Some(n) => i = n,
                None => break,
            }
        }
        i
    }

    /// Click on step `i`. Modulators and aggregate internals select the enclosing step, with a note.
    pub fn jump(&mut self, i: usize) {
        if i >= self.steps.len() {
            return;
        }
        let target = self.navigable(i);
        self.set(target);
        if target != i {
            self.note = Some(format!(
                "`{}()` is evaluated as part of `{}()`, so its results are shown there.",
                self.steps[i].name, self.steps[target].name
            ));
        }
    }

    /// The step whose source range contains `offset` (innermost).
    pub fn step_at(&self, offset: usize) -> Option<usize> {
        self.steps
            .iter()
            .enumerate()
            .filter(|(_, s)| s.span.contains(&offset))
            .max_by_key(|(_, s)| s.depth)
            .map(|(i, _)| i)
    }

    /// Steps in the main traversal, in order.
    pub fn main_chain(&self) -> &[usize] {
        &self.chains[0]
    }

    /// The main-traversal step that contains step `i` (itself when it is in the main traversal).
    pub fn main_ancestor(&self, mut i: usize) -> usize {
        while let Some(p) = self.steps[i].parent {
            i = p;
        }
        i
    }
}

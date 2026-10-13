//! The queries a debug session runs. All are read-only by construction: prefixes of a statement that
//! was checked not to mutate, with limits added (ADR-0020).

use crate::session::Session;

/// Steps after which per-element simulation cannot tell which elements survived (they merge
/// traversers or drop path history).
const REDUCERS: [&str; 13] = [
    "count",
    "fold",
    "sum",
    "mean",
    "min",
    "max",
    "group",
    "groupCount",
    "tree",
    "cap",
    "aggregate",
    "barrier",
    "math",
];

/// The binding name for element ids in simulation and inspection queries.
pub const IDS: &str = "dbgIds";

/// Elements the simulation can trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Vertex,
    Edge,
}

impl ElementKind {
    fn source(self) -> &'static str {
        match self {
            ElementKind::Vertex => "V",
            ElementKind::Edge => "E",
        }
    }
}

fn join(before: &str, call: &str) -> String {
    if before.is_empty() {
        call.to_owned()
    } else {
        format!("{before}.{call}")
    }
}

impl Session {
    /// The steps of chain `chain` up to and including `upto`, joined (`out('a').has('b')`).
    fn chain_text(&self, chain: usize, upto: usize) -> String {
        let mut out = String::new();
        for &s in &self.chains()[chain] {
            out = join(&out, self.call_text(s));
            if s == upto {
                break;
            }
        }
        out
    }

    /// The traversal before step `i` in its own chain, in its full context.
    fn before(&self, i: usize) -> String {
        let step = self.step(i);
        let chain = &self.chains()[step.chain];
        let pos = chain.iter().position(|&s| s == i).unwrap_or(0);
        match (pos, step.parent) {
            (0, None) => "g".to_owned(),
            (0, Some(parent)) => self.before(parent),
            (_, _) => self.prefix(chain[pos - 1]),
        }
    }

    /// An executable traversal up to and including step `i`. A step inside a nested traversal runs in
    /// its parent's context: the steps before the parent, then `flatMap()` over the nested traversal up
    /// to the step (one iteration for a `repeat()` body).
    pub fn prefix(&self, i: usize) -> String {
        let step = self.step(i);
        match step.parent {
            None => join(&self.before(i), self.call_text(i)),
            Some(parent) => {
                let context = self.before(parent);
                let nested = self.chain_text(step.chain, i);
                let nested = nested.strip_prefix("__.").unwrap_or(&nested);
                join(&context, &format!("flatMap(__.{nested})"))
            }
        }
    }

    /// A note for prefixes that approximate the real evaluation.
    pub fn prefix_note(&self, i: usize) -> Option<String> {
        let parent = self.step(i).parent?;
        (self.step(parent).name == "repeat")
            .then(|| "Inside repeat(), results are for the first iteration.".to_owned())
    }

    /// Results at step `i`, at most `limit`.
    pub fn results_query(&self, i: usize, limit: usize) -> String {
        format!("{}.limit({limit})", self.prefix(i))
    }

    /// Traversers after step `i`, counting at most `cap`.
    pub fn count_query(&self, i: usize, cap: u64) -> String {
        format!("{}.limit({cap}).count()", self.prefix(i))
    }

    /// The main-traversal steps after step `i` (the main step containing it).
    fn rest(&self, i: usize) -> Vec<usize> {
        let main = self.main_ancestor(i);
        let chain = self.main_chain();
        let pos = chain.iter().position(|&s| s == main).unwrap_or(chain.len());
        chain[pos + 1..].to_vec()
    }

    /// Why the simulation cannot run from step `i`, if it cannot.
    pub fn simulation_unavailable(&self, i: usize) -> Option<String> {
        self.rest(i)
            .iter()
            .find(|&&s| REDUCERS.contains(&self.step(s).name.as_str()))
            .map(|&s| {
                format!(
                    "`{}()` later in the traversal combines traversers, so individual elements cannot be traced through it.",
                    self.step(s).name
                )
            })
    }

    /// Which of the elements bound to [`IDS`] (taken at step `i`) lead to a final result: their ids.
    pub fn survivors_query(&self, i: usize, kind: ElementKind) -> String {
        let rest: Vec<&str> = self.rest(i).iter().map(|&s| self.call_text(s)).collect();
        if rest.is_empty() {
            return format!("g.{}({IDS}).id()", kind.source());
        }
        format!(
            "g.{}({IDS}).as('dbgStart').{}.select('dbgStart').dedup().id()",
            kind.source(),
            rest.join(".")
        )
    }

    /// For one element (bound to [`IDS`]) taken at step `i`: one query per remaining main step, each
    /// counting whether the element is still there after it (0 or 1). The first 0 is where it drops.
    pub fn inspection_queries(&self, i: usize, kind: ElementKind) -> Vec<(usize, String)> {
        let mut acc = format!("g.{}({IDS})", kind.source());
        self.rest(i)
            .into_iter()
            .map(|s| {
                acc = join(&acc, self.call_text(s));
                (s, format!("{acc}.limit(1).count()"))
            })
            .collect()
    }
}

/// `1204` → `1,204`; at the cap, `≥ 100,000`.
pub fn format_count(n: u64, cap: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n >= cap { format!("≥ {out}") } else { out }
}

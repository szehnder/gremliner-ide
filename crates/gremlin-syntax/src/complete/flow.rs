//! Label flow: which element labels can be at a point in a traversal, walking its steps left to right
//! over the schema's connectivity triples (design: autocomplete). Linear traversals are exact enough to
//! scope property keys and edge labels; anything it does not understand widens to "any".

use std::collections::BTreeSet;

use schema::{Schema, Triple};

use super::scan::{Call, Chain, Level};

/// The traverser type and its possible labels (`None`: any label).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flow {
    /// At `g` (no step yet).
    Start,
    Vertices(Option<BTreeSet<String>>),
    /// Edges, as the triples they can be, and the direction they were reached in (for `otherV`).
    Edges {
        triples: Option<Vec<Triple>>,
        came: Dir,
    },
    /// Values, maps, paths, … (or not understood).
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Out,
    In,
    Both,
}

impl Flow {
    /// Vertex labels, when the flow is vertices with known labels.
    pub fn vertex_labels(&self) -> Option<&BTreeSet<String>> {
        match self {
            Flow::Vertices(Some(set)) => Some(set),
            _ => None,
        }
    }

    /// Edge labels, when the flow is edges with known labels.
    pub fn edge_labels(&self) -> Option<BTreeSet<String>> {
        match self {
            Flow::Edges {
                triples: Some(t), ..
            } => Some(t.iter().map(|t| t.label.clone()).collect()),
            _ => None,
        }
    }

    pub fn is_edges(&self) -> bool {
        matches!(self, Flow::Edges { .. })
    }
}

/// Steps that pass their input through unchanged (filters, modulators, side effects).
const KEEP: &[&str] = &[
    "has",
    "hasId",
    "hasKey",
    "hasValue",
    "hasNot",
    "where",
    "filter",
    "not",
    "and",
    "or",
    "is",
    "dedup",
    "limit",
    "range",
    "skip",
    "tail",
    "sample",
    "coin",
    "simplePath",
    "cyclicPath",
    "order",
    "by",
    "as",
    "aggregate",
    "store",
    "sideEffect",
    "barrier",
    "timeLimit",
    "emit",
    "until",
    "times",
    "identity",
    "property",
    "from",
    "to",
    "option",
    "with",
    "subgraph",
    "profile",
    "none",
    "discard",
    "drop",
    "fail",
    "all",
    "any",
];

/// Steps that modulate the step before them (`by`, `as`, …): skipped when finding what `by` refers to.
const MODULATORS: &[&str] = &[
    "by", "as", "with", "option", "from", "to", "times", "until", "emit",
];

/// Steps whose result keeps the element type but loses label precision.
const WIDEN: &[&str] = &[
    "repeat", "union", "coalesce", "optional", "local", "choose", "branch", "flatMap", "map",
    "match",
];

fn strings(call: &Call) -> Vec<String> {
    call.args.iter().filter_map(|a| a.string.clone()).collect()
}

fn set(labels: Vec<String>) -> Option<BTreeSet<String>> {
    (!labels.is_empty()).then(|| labels.into_iter().collect())
}

/// Edges leaving (`Out`), entering (`In`) or touching (`Both`) `from`, limited to `labels`.
fn edges_from(
    schema: Option<&Schema>,
    from: Option<&BTreeSet<String>>,
    labels: &[String],
    dir: Dir,
) -> Option<Vec<Triple>> {
    let schema = schema?;
    let triples = schema
        .triples
        .iter()
        .filter(|t| labels.is_empty() || labels.contains(&t.label))
        .filter(|t| {
            from.is_none_or(|f| match dir {
                Dir::Out => f.contains(&t.out_label),
                Dir::In => f.contains(&t.in_label),
                Dir::Both => f.contains(&t.out_label) || f.contains(&t.in_label),
            })
        })
        .cloned()
        .collect();
    Some(triples)
}

/// The vertex labels at the far end of `triples`, travelling in `dir`.
fn far_ends(triples: &[Triple], dir: Dir) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for t in triples {
        match dir {
            Dir::Out => {
                out.insert(t.in_label.clone());
            }
            Dir::In => {
                out.insert(t.out_label.clone());
            }
            Dir::Both => {
                out.insert(t.in_label.clone());
                out.insert(t.out_label.clone());
            }
        }
    }
    out
}

fn vertex_step(flow: &Flow, call: &Call, schema: Option<&Schema>, dir: Dir) -> Flow {
    let labels = strings(call);
    match flow {
        Flow::Vertices(from) => match edges_from(schema, from.as_ref(), &labels, dir) {
            Some(triples) => Flow::Vertices(Some(far_ends(&triples, dir))),
            None => Flow::Vertices(None),
        },
        _ => Flow::Vertices(None),
    }
}

fn edge_step(flow: &Flow, call: &Call, schema: Option<&Schema>, dir: Dir) -> Flow {
    let labels = strings(call);
    let from = match flow {
        Flow::Vertices(f) => f.clone(),
        _ => None,
    };
    let triples = edges_from(schema, from.as_ref(), &labels, dir);
    Flow::Edges { triples, came: dir }
}

/// Apply one step.
pub fn step(flow: &Flow, call: &Call, schema: Option<&Schema>) -> Flow {
    let name = call.name.as_str();
    match name {
        "V" => Flow::Vertices(None),
        "E" => Flow::Edges {
            triples: None,
            came: Dir::Both,
        },
        "addV" | "mergeV" => Flow::Vertices(set(strings(call))),
        "addE" => {
            let labels = strings(call);
            let triples = schema.map(|s| {
                s.triples
                    .iter()
                    .filter(|t| labels.contains(&t.label))
                    .cloned()
                    .collect()
            });
            Flow::Edges {
                triples: triples.filter(|t: &Vec<Triple>| !t.is_empty()),
                came: Dir::Out,
            }
        }
        "hasLabel" => {
            let labels = strings(call);
            if labels.is_empty() {
                return flow.clone();
            }
            match flow {
                Flow::Edges { triples, came } => Flow::Edges {
                    triples: match triples {
                        Some(t) => Some(
                            t.iter()
                                .filter(|t| labels.contains(&t.label))
                                .cloned()
                                .collect(),
                        ),
                        None => schema.map(|s| {
                            s.triples
                                .iter()
                                .filter(|t| labels.contains(&t.label))
                                .cloned()
                                .collect()
                        }),
                    },
                    came: *came,
                },
                _ => Flow::Vertices(set(labels)),
            }
        }
        // has(label, key, value) restricts the label.
        "has" if call.args.len() == 3 && call.args[0].string.is_some() => {
            let label = call.args[0].string.clone().unwrap_or_default();
            match flow {
                Flow::Edges { .. } => flow.clone(),
                _ => Flow::Vertices(set(vec![label])),
            }
        }
        "out" => vertex_step(flow, call, schema, Dir::Out),
        "in" => vertex_step(flow, call, schema, Dir::In),
        "both" => vertex_step(flow, call, schema, Dir::Both),
        "outE" => edge_step(flow, call, schema, Dir::Out),
        "inE" => edge_step(flow, call, schema, Dir::In),
        "bothE" => edge_step(flow, call, schema, Dir::Both),
        "inV" | "outV" | "otherV" | "bothV" => match flow {
            Flow::Edges {
                triples: Some(t),
                came,
            } => {
                let dir = match (name, came) {
                    ("inV", _) => Dir::Out,
                    ("outV", _) => Dir::In,
                    ("otherV", Dir::Out) => Dir::Out,
                    ("otherV", Dir::In) => Dir::In,
                    _ => Dir::Both,
                };
                Flow::Vertices(Some(far_ends(t, dir)))
            }
            _ => Flow::Vertices(None),
        },
        _ if KEEP.contains(&name) => flow.clone(),
        _ if WIDEN.contains(&name) => match flow {
            Flow::Vertices(_) => Flow::Vertices(None),
            Flow::Edges { came, .. } => Flow::Edges {
                triples: None,
                came: *came,
            },
            other => other.clone(),
        },
        _ => Flow::Other,
    }
}

/// Run `calls` from `start`.
pub fn run(calls: &[Call], start: Flow, schema: Option<&Schema>) -> Flow {
    calls
        .iter()
        .fold(start, |flow, call| step(&flow, call, schema))
}

fn start_of(chain: &Chain, inherited: &Flow) -> Flow {
    if chain.is_anonymous() {
        inherited.clone()
    } else if chain.root.as_deref() == Some("g") {
        Flow::Start
    } else {
        Flow::Other
    }
}

/// What the arguments of `call` see, given the chain before it.
fn args_flow(chain: &Chain, call: &Call, inherited: &Flow, schema: Option<&Schema>) -> Flow {
    let start = start_of(chain, inherited);
    if call.name == "by" {
        // `by` modulates the closest non-modulator step: its arguments see that step's input.
        let calls = &chain.calls;
        let mut j = calls.len();
        while j > 0 && MODULATORS.contains(&calls[j - 1].name.as_str()) {
            j -= 1;
        }
        return run(&calls[..j.saturating_sub(1)], start, schema);
    }
    run(&chain.calls, start, schema)
}

/// The flow seen by the arguments of the innermost call in `levels`.
pub fn at_args(levels: &[Level], schema: Option<&Schema>) -> Flow {
    let mut inherited = Flow::Other;
    for level in levels {
        inherited = args_flow(&level.chain, &level.call, &inherited, schema);
    }
    inherited
}

/// The flow after `chain`, written inside `levels`.
pub fn after_chain(levels: &[Level], chain: &Chain, schema: Option<&Schema>) -> Flow {
    let inherited = at_args(levels, schema);
    run(&chain.calls, start_of(chain, &inherited), schema)
}

/// Step labels declared with `as('x')` in the chains leading to the cursor.
pub fn step_labels(levels: &[Level], chain: Option<&Chain>) -> Vec<String> {
    let mut out = Vec::new();
    let chains = levels.iter().map(|l| &l.chain).chain(chain);
    for c in chains {
        for call in c.calls.iter().filter(|c| c.name == "as") {
            for s in strings(call) {
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::complete::scan::{Tail, scan};

    fn air() -> Schema {
        Schema::from_json(include_str!("../../tests/fixtures/air-routes-schema.json"))
            .expect("fixture")
    }

    fn flow_after(text: &str, schema: &Schema) -> Flow {
        let s = scan(text, false);
        let Tail::AfterDot { chain } = &s.tail else {
            panic!("{s:?}")
        };
        after_chain(&s.levels, chain, Some(schema))
    }

    fn labels(f: &Flow) -> Vec<String> {
        f.vertex_labels()
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    #[test]
    fn walks_out_in_and_edges() {
        let s = air();
        assert_eq!(
            labels(&flow_after(
                "g.V().hasLabel('country').out('contains').",
                &s
            )),
            ["airport"]
        );
        assert_eq!(
            labels(&flow_after("g.V().hasLabel('airport').in('contains').", &s)),
            ["continent", "country"]
        );
        let f = flow_after("g.V().hasLabel('airport').outE('route').", &s);
        assert_eq!(
            f.edge_labels().unwrap().into_iter().collect::<Vec<_>>(),
            ["route"]
        );
        assert_eq!(
            labels(&flow_after("g.V().hasLabel('airport').outE().inV().", &s)),
            ["airport"]
        );
        assert_eq!(
            labels(&flow_after(
                "g.V().has('country','code','US').outE().otherV().",
                &s
            )),
            ["airport"]
        );
        assert_eq!(flow_after("g.V().", &s), Flow::Vertices(None));
        assert_eq!(flow_after("g.V().values('code').", &s), Flow::Other);
    }

    #[test]
    fn nested_and_by_inherit() {
        let s = air();
        // Inside where(): continues from airport.
        assert_eq!(
            labels(&flow_after(
                "g.V().hasLabel('airport').where(out('route').",
                &s
            )),
            ["airport"]
        );
        // by() sees the input of the step it modulates.
        let scanned = scan("g.V().hasLabel('airport').group().by('", false);
        assert_eq!(labels(&at_args(&scanned.levels, Some(&s))), ["airport"]);
        let scanned = scan(
            "g.V().hasLabel('airport').order().by('runways').by('",
            false,
        );
        assert_eq!(labels(&at_args(&scanned.levels, Some(&s))), ["airport"]);
    }

    #[test]
    fn as_labels_are_collected() {
        let scanned = scan("g.V().as('a').out().as('b').select('", false);
        assert_eq!(step_labels(&scanned.levels, None), ["a", "b"]);
    }
}

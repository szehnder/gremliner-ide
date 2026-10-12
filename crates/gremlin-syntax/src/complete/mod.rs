//! Context-aware completion, signature help and schema warnings for the query editor (design:
//! autocomplete). Everything here is synchronous and allocation-light: it runs on each keystroke.

pub mod flow;
pub mod scan;

use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::LazyLock;

use schema::Schema;
use serde::Deserialize;

use crate::catalogue::{Catalogue, StepKind};
use flow::Flow;
use scan::{Arg, Level, Tail, scan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Step,
    VertexLabel,
    EdgeLabel,
    PropertyKey,
    Value,
    Predicate,
    Enum,
    StepLabel,
    Snippet,
    Keyword,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub kind: ItemKind,
    /// One line shown next to the label (a description, type or origin).
    pub detail: String,
    /// What replaces [`Completions::replace`].
    pub insert: String,
    /// Where the cursor goes within `insert` (byte offset); `None` for the end.
    pub cursor: Option<usize>,
    /// Higher sorts first among equally good matches.
    pub boost: u8,
}

impl Item {
    fn new(label: impl Into<String>, kind: ItemKind, detail: impl Into<String>) -> Self {
        let label = label.into();
        Self {
            insert: label.clone(),
            label,
            kind,
            detail: detail.into(),
            cursor: None,
            boost: 0,
        }
    }

    fn insert(mut self, text: impl Into<String>, cursor: Option<usize>) -> Self {
        self.insert = text.into();
        self.cursor = cursor;
        self
    }

    fn boost(mut self, boost: u8) -> Self {
        self.boost = boost;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Completions {
    /// The text the chosen item replaces (the partial word or string content).
    pub replace: Range<usize>,
    pub items: Vec<Item>,
}

impl Completions {
    pub fn labels(&self) -> Vec<&str> {
        self.items.iter().map(|i| i.label.as_str()).collect()
    }
}

/// Steps valid directly on `g`.
pub const SOURCE_STEPS: &[&str] = &[
    "V",
    "E",
    "addV",
    "addE",
    "inject",
    "mergeV",
    "mergeE",
    "call",
    "io",
    "with",
    "withBulk",
    "withPath",
    "withSack",
    "withSideEffect",
    "withStrategies",
    "withoutStrategies",
    "withComputer",
    "tx",
];

/// Only valid on `g` (never mid-traversal).
const SOURCE_ONLY: &[&str] = &[
    "io",
    "withBulk",
    "withPath",
    "withSack",
    "withSideEffect",
    "withStrategies",
    "withoutStrategies",
    "withComputer",
    "tx",
];

/// Frequently used steps, ranked above the rest.
const COMMON: &[&str] = &[
    "has",
    "hasLabel",
    "out",
    "in",
    "both",
    "outE",
    "inE",
    "bothE",
    "inV",
    "outV",
    "otherV",
    "values",
    "valueMap",
    "elementMap",
    "count",
    "limit",
    "dedup",
    "where",
    "select",
    "project",
    "by",
    "order",
    "group",
    "groupCount",
    "path",
    "repeat",
    "until",
    "times",
    "as",
    "fold",
    "unfold",
    "id",
    "label",
    "V",
    "E",
    "addV",
    "addE",
    "property",
    "toList",
    "next",
];

/// Members of enum-like classes, with descriptions.
const ENUMS: &[(&str, &[(&str, &str)])] = &[
    (
        "P",
        &[
            ("eq", "equal to"),
            ("neq", "not equal to"),
            ("lt", "less than"),
            ("lte", "less than or equal to"),
            ("gt", "greater than"),
            ("gte", "greater than or equal to"),
            ("inside", "strictly between two values"),
            ("outside", "outside two values"),
            ("between", "at least the first, less than the second"),
            ("within", "one of the values"),
            ("without", "none of the values"),
            ("not", "negates a predicate"),
        ],
    ),
    (
        "TextP",
        &[
            ("containing", "contains the text"),
            ("notContaining", "does not contain the text"),
            ("startingWith", "starts with the text"),
            ("notStartingWith", "does not start with the text"),
            ("endingWith", "ends with the text"),
            ("notEndingWith", "does not end with the text"),
            ("regex", "matches the regular expression"),
            ("notRegex", "does not match the regular expression"),
        ],
    ),
    (
        "T",
        &[
            ("id", "element id"),
            ("label", "element label"),
            ("key", "property key"),
            ("value", "property value"),
        ],
    ),
    (
        "Order",
        &[
            ("asc", "ascending"),
            ("desc", "descending"),
            ("shuffle", "random order"),
        ],
    ),
    (
        "Scope",
        &[
            ("local", "within each collection"),
            ("global", "across all traversers"),
        ],
    ),
    ("Column", &[("keys", "map keys"), ("values", "map values")]),
    (
        "Pop",
        &[
            ("first", "first match"),
            ("last", "last match"),
            ("all", "every match"),
            ("mixed", "single or list as found"),
        ],
    ),
    (
        "Cardinality",
        &[
            ("single", "replace the value"),
            ("list", "add a value"),
            ("set", "add a distinct value"),
        ],
    ),
    (
        "Direction",
        &[
            ("OUT", "outgoing"),
            ("IN", "incoming"),
            ("BOTH", "either"),
            ("from", "out end (mergeE)"),
            ("to", "in end (mergeE)"),
        ],
    ),
    (
        "Merge",
        &[
            ("onCreate", "when creating"),
            ("onMatch", "when matched"),
            ("outV", "out vertex"),
            ("inV", "in vertex"),
        ],
    ),
    (
        "Pick",
        &[("any", "any value"), ("none", "no option matched")],
    ),
    (
        "Operator",
        &[
            ("sum", "add"),
            ("minus", "subtract"),
            ("mult", "multiply"),
            ("div", "divide"),
            ("min", "minimum"),
            ("max", "maximum"),
            ("assign", "replace"),
            ("addAll", "add all to a collection"),
            ("sumLong", "add as long"),
        ],
    ),
    (
        "DT",
        &[
            ("second", "seconds"),
            ("minute", "minutes"),
            ("hour", "hours"),
            ("day", "days"),
        ],
    ),
    (
        "WithOptions",
        &[
            ("tokens", "include id and label"),
            ("none", "no tokens"),
            ("ids", "include ids"),
            ("labels", "include labels"),
            ("keys", "include keys"),
            ("values", "include values"),
            ("all", "include everything"),
        ],
    ),
];

/// Bare tokens usable as arguments (statically imported).
const ARG_TOKENS: &[(&str, &str)] = &[
    ("desc", "Order.desc"),
    ("asc", "Order.asc"),
    ("shuffle", "Order.shuffle"),
    ("local", "Scope.local"),
    ("global", "Scope.global"),
    ("keys", "Column.keys"),
    ("values", "Column.values"),
    ("first", "Pop.first"),
    ("last", "Pop.last"),
    ("single", "Cardinality.single"),
    ("list", "Cardinality.list"),
    ("set", "Cardinality.set"),
    ("OUT", "Direction.OUT"),
    ("IN", "Direction.IN"),
    ("BOTH", "Direction.BOTH"),
];

#[derive(Debug, Clone, Deserialize)]
pub struct Snippet {
    pub trigger: String,
    pub description: String,
    /// The text to insert, `|` marking the cursor.
    pub body: String,
}

#[derive(Deserialize)]
struct SnippetFile {
    snippet: Vec<Snippet>,
}

static SNIPPETS: LazyLock<Vec<Snippet>> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    let file: SnippetFile =
        toml::from_str(include_str!("../../snippets.toml")).expect("snippets.toml is valid");
    file.snippet
});

pub fn snippets() -> &'static [Snippet] {
    &SNIPPETS
}

fn first_sentence(doc: &str) -> String {
    match doc.find(". ") {
        Some(i) => doc[..=i].to_owned(),
        None => doc.to_owned(),
    }
}

/// How well `label` matches what was typed: 0 case-sensitive prefix, 1 prefix ignoring case, 2 fuzzy
/// (subsequence ignoring case); `None` when it does not match.
pub fn match_class(label: &str, typed: &str) -> Option<u8> {
    if typed.is_empty() || label.starts_with(typed) {
        return Some(0);
    }
    let l = label.to_lowercase();
    let t = typed.to_lowercase();
    if l.starts_with(&t) {
        return Some(1);
    }
    let mut chars = l.chars();
    t.chars()
        .all(|c| chars.by_ref().any(|x| x == c))
        .then_some(2)
}

/// Filter by what was typed, then order by match quality, boost and the original order.
fn rank(items: Vec<Item>, typed: &str, filter_on: impl Fn(&Item) -> String) -> Vec<Item> {
    let mut scored: Vec<(u8, u8, usize, Item)> = items
        .into_iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let class = match_class(&filter_on(&item), typed)?;
            // Values are data, not names: only prefixes count.
            if item.kind == ItemKind::Value && class > 1 {
                return None;
            }
            Some((class, item.boost, i, item))
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    let mut seen = BTreeSet::new();
    scored
        .into_iter()
        .map(|(_, _, _, item)| item)
        .filter(|item| seen.insert((item.label.clone(), item.kind as u8)))
        .collect()
}

fn step_item(name: &str, boost_common: bool) -> Option<Item> {
    let info = Catalogue::get().step(name)?;
    let zero_arg = info.signatures.iter().any(|s| s.ends_with("()"));
    let insert = format!("{name}()");
    let cursor = (!zero_arg).then_some(name.len() + 1);
    let boost = if boost_common && COMMON.contains(&name) {
        2
    } else {
        0
    };
    Some(
        Item::new(name, ItemKind::Step, first_sentence(&info.doc))
            .insert(insert, cursor)
            .boost(boost),
    )
}

fn snippet_items() -> Vec<Item> {
    snippets()
        .iter()
        .map(|s| {
            let cursor = s.body.find('|');
            Item::new(s.trigger.clone(), ItemKind::Snippet, s.description.clone())
                .insert(s.body.replace('|', ""), cursor)
        })
        .collect()
}

/// What an argument is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// A vertex label (or an edge label when the flow is edges).
    Label,
    VertexLabel,
    EdgeLabel,
    /// A property key, of `label` when given (else of the flow's labels).
    PropertyKey {
        label: Option<String>,
    },
    /// A value of `key`, on `label` when given.
    Value {
        key: Option<String>,
        label: Option<String>,
    },
    /// A step label from `as()`.
    StepLabel,
    None,
}

fn is_key_anywhere(schema: Option<&Schema>, key: &str) -> bool {
    schema.is_some_and(|s| {
        s.vertex_labels
            .iter()
            .chain(&s.edge_labels)
            .any(|l| l.property(key).is_some())
    })
}

fn is_label_anywhere(schema: Option<&Schema>, label: &str) -> bool {
    schema.is_some_and(|s| s.knows_label(label))
}

/// The role of argument `index` of `step`, given the arguments before it.
pub fn role(step: &str, index: usize, args: &[Arg], schema: Option<&Schema>) -> Role {
    let arg = |i: usize| args.get(i).and_then(|a| a.string.clone());
    match step {
        "hasLabel" => Role::Label,
        "addV" => Role::VertexLabel,
        "addE" => Role::EdgeLabel,
        "out" | "in" | "both" | "outE" | "inE" | "bothE" => Role::EdgeLabel,
        "to" if index >= 1 && args.first().is_some_and(|a| a.string.is_none()) => Role::EdgeLabel,
        "has" => match index {
            0 => Role::PropertyKey { label: None },
            1 => match arg(0) {
                Some(first)
                    if !is_key_anywhere(schema, &first) && is_label_anywhere(schema, &first) =>
                {
                    Role::PropertyKey { label: Some(first) }
                }
                key => Role::Value { key, label: None },
            },
            2 => Role::Value {
                key: arg(1),
                label: arg(0),
            },
            _ => Role::None,
        },
        // `valueMap(true, 'k')`: the boolean is never a string, so string positions are keys.
        "values" | "properties" | "valueMap" | "elementMap" | "propertyMap" | "hasKey"
        | "hasNot" => Role::PropertyKey { label: None },
        "by" if index == 0 => Role::PropertyKey { label: None },
        "property" => {
            let card = args
                .first()
                .is_some_and(|a| a.string.is_none() && is_cardinality(&a.raw));
            let key_ix = usize::from(card);
            if index == key_ix {
                Role::PropertyKey { label: None }
            } else if index == key_ix + 1 {
                Role::Value {
                    key: arg(key_ix),
                    label: None,
                }
            } else {
                Role::None
            }
        }
        "select" | "dedup" | "cap" => Role::StepLabel,
        "where" | "from" | "to" if index == 0 => Role::StepLabel,
        _ => Role::None,
    }
}

fn is_cardinality(raw: &str) -> bool {
    let raw = raw.trim();
    let tail = raw.strip_prefix("Cardinality.").unwrap_or(raw);
    matches!(tail, "single" | "list" | "set")
}

/// Labels the flow could be, as a list (empty: unknown).
fn flow_vertex_labels(flow: &Flow) -> Vec<String> {
    flow.vertex_labels()
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default()
}

/// Property keys for the flow: (key, type text, owning labels), the flow's own first.
fn keys_for(
    schema: &Schema,
    flow: &Flow,
    label: Option<&str>,
) -> Vec<(String, String, Vec<String>)> {
    let labels: Vec<&schema::LabelInfo> = match (label, flow) {
        (Some(l), Flow::Edges { .. }) => schema.edge_label(l).into_iter().collect(),
        (Some(l), _) => schema
            .vertex_label(l)
            .or_else(|| schema.edge_label(l))
            .into_iter()
            .collect(),
        (None, Flow::Vertices(Some(set))) => schema
            .vertex_labels
            .iter()
            .filter(|l| set.contains(&l.name))
            .collect(),
        (None, Flow::Vertices(None)) => schema.vertex_labels.iter().collect(),
        (None, Flow::Edges { .. }) => match flow.edge_labels() {
            Some(set) => schema
                .edge_labels
                .iter()
                .filter(|l| set.contains(&l.name))
                .collect(),
            None => schema.edge_labels.iter().collect(),
        },
        (None, _) => schema
            .vertex_labels
            .iter()
            .chain(&schema.edge_labels)
            .collect(),
    };
    let mut out: Vec<(String, String, Vec<String>)> = Vec::new();
    for l in labels {
        for p in &l.properties {
            match out.iter_mut().find(|(k, _, _)| *k == p.key) {
                Some((_, _, owners)) => owners.push(l.name.clone()),
                None => out.push((p.key.clone(), p.type_text(), vec![l.name.clone()])),
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Sample values of `key` (on `label`, else on the flow's labels, else anywhere).
fn samples_for(
    schema: &Schema,
    flow: &Flow,
    key: &str,
    label: Option<&str>,
) -> Vec<(String, bool, String)> {
    let flow_labels = flow_vertex_labels(flow);
    let owners: Vec<&schema::LabelInfo> = schema
        .vertex_labels
        .iter()
        .chain(&schema.edge_labels)
        .filter(|l| match label {
            Some(want) => l.name == want,
            None => flow_labels.is_empty() || flow_labels.contains(&l.name),
        })
        .collect();
    let mut out = Vec::new();
    for l in owners {
        if let Some(p) = l.property(key) {
            let is_string = p.primary_type() == Some("String");
            for s in &p.samples {
                if !out.iter().any(|(v, _, _): &(String, bool, String)| v == s) {
                    out.push((s.clone(), is_string, format!("{}.{key}", l.name)));
                }
            }
        }
    }
    out
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn escape_in_string(value: &str, quote: char) -> String {
    let mut out = value.replace('\\', "\\\\");
    if quote == '\'' {
        out = out.replace('\'', "\\'");
    } else {
        out = out.replace('"', "\\\"");
    }
    out
}

/// Items for a role. `quoted`: the cursor is not inside a string, so string items get quotes.
fn role_items(
    role: &Role,
    flow: &Flow,
    levels: &[Level],
    schema: Option<&Schema>,
    quoted: bool,
) -> Vec<Item> {
    let wrap = |item: Item| {
        if quoted {
            let text = quote(&item.insert);
            item.insert(text, None)
        } else {
            item
        }
    };
    let mut out = Vec::new();
    match role {
        Role::StepLabel => {
            for l in flow::step_labels(levels, None) {
                out.push(wrap(Item::new(
                    l,
                    ItemKind::StepLabel,
                    "step label from as()",
                )));
            }
        }
        _ if schema.is_none() => {}
        Role::Label | Role::VertexLabel | Role::EdgeLabel => {
            let schema = schema.unwrap_or_else(|| unreachable!());
            let edges =
                matches!(role, Role::EdgeLabel) || (matches!(role, Role::Label) && flow.is_edges());
            if edges {
                // Rank edge labels reachable from the current vertex labels first.
                let from = flow.vertex_labels();
                let step = levels.last().map(|l| l.call.name.as_str()).unwrap_or("");
                let reachable = |name: &str| -> bool {
                    let Some(from) = from else { return true };
                    schema.triples.iter().any(|t| {
                        t.label == name
                            && match step {
                                "in" | "inE" => from.contains(&t.in_label),
                                "both" | "bothE" => {
                                    from.contains(&t.in_label) || from.contains(&t.out_label)
                                }
                                _ => from.contains(&t.out_label),
                            }
                    })
                };
                for l in &schema.edge_labels {
                    let ok = reachable(&l.name);
                    let detail = match (&l.count, ok, from) {
                        (_, false, Some(from)) => format!(
                            "edge label \u{b7} not from {}",
                            from.iter().cloned().collect::<Vec<_>>().join(", ")
                        ),
                        (Some(c), _, _) => format!("edge label \u{b7} {}", c.text()),
                        _ => "edge label".to_owned(),
                    };
                    out.push(wrap(
                        Item::new(l.name.clone(), ItemKind::EdgeLabel, detail).boost(if ok {
                            3
                        } else {
                            0
                        }),
                    ));
                }
            } else {
                for l in &schema.vertex_labels {
                    let detail = match &l.count {
                        Some(c) => format!("vertex label \u{b7} {}", c.text()),
                        None => "vertex label".to_owned(),
                    };
                    out.push(wrap(Item::new(
                        l.name.clone(),
                        ItemKind::VertexLabel,
                        detail,
                    )));
                }
            }
        }
        Role::PropertyKey { label } => {
            let schema = schema.unwrap_or_else(|| unreachable!());
            for (key, ty, owners) in keys_for(schema, flow, label.as_deref()) {
                let detail = format!("{ty} \u{b7} {}", owners.join(", "));
                out.push(wrap(Item::new(key, ItemKind::PropertyKey, detail)));
            }
        }
        Role::Value { key, label } => {
            let schema = schema.unwrap_or_else(|| unreachable!());
            if let Some(key) = key {
                for (value, is_string, origin) in samples_for(schema, flow, key, label.as_deref()) {
                    let item = Item::new(value, ItemKind::Value, format!("sample of {origin}"));
                    out.push(if is_string { wrap(item) } else { item });
                }
            }
        }
        Role::None => {}
    }
    out
}

/// Completions at `offset` in `text`.
pub fn complete(text: &str, offset: usize, schema: Option<&Schema>) -> Completions {
    let offset = offset.min(text.len());
    let offset = (0..=offset)
        .rev()
        .find(|&o| text.is_char_boundary(o))
        .unwrap_or(0);
    let prefix = &text[..offset];
    let scanned = scan(prefix, false);
    let typed = scanned.partial.clone();
    let replace = scanned.replace.clone();
    let items = match &scanned.tail {
        Tail::AfterDot { chain } => {
            let root = chain.root.as_deref();
            let members = ENUMS.iter().find(|(class, _)| Some(*class) == root);
            match members {
                Some((class, members)) if chain.calls.is_empty() => {
                    rank(enum_items(class, members), &typed, |i| i.label.clone())
                }
                _ if root == Some("g") && chain.calls.is_empty() => {
                    let items = SOURCE_STEPS
                        .iter()
                        .filter_map(|s| step_item(s, true))
                        .collect();
                    rank(items, &typed, |i| i.label.clone())
                }
                _ => steps_after(&typed),
            }
        }
        Tail::InString { args, index } => {
            let Some(level) = scanned.levels.last() else {
                return Completions::default();
            };
            let flow = flow::at_args(&scanned.levels, schema);
            let role = role(&level.call.name, *index, args, schema);
            let quote_char = text[..replace.start].chars().last().unwrap_or('\'');
            let closes = text[offset..].starts_with(quote_char);
            let items = role_items(&role, &flow, &scanned.levels, schema, false)
                .into_iter()
                .map(|item| {
                    let mut insert = escape_in_string(&item.insert, quote_char);
                    if !closes {
                        insert.push(quote_char);
                    }
                    item.insert(insert, None)
                })
                .collect();
            rank(items, &typed, |i| i.label.clone())
        }
        Tail::InArg { args, index } => {
            let Some(level) = scanned.levels.last() else {
                return Completions::default();
            };
            let flow = flow::at_args(&scanned.levels, schema);
            let role = role(&level.call.name, *index, args, schema);
            let mut items = Vec::new();
            if typed.is_empty() {
                items.extend(role_items(&role, &flow, &scanned.levels, schema, true));
            } else {
                // Typing a word: tokens, predicates and anonymous steps.
                if matches!(role, Role::Value { .. }) {
                    items.extend(
                        role_items(&role, &flow, &scanned.levels, schema, true)
                            .into_iter()
                            .filter(|i| !i.insert.starts_with('\'')),
                    );
                }
                for (name, desc) in ENUMS[0].1.iter().chain(ENUMS[1].1) {
                    let insert = format!("{name}()");
                    items.push(
                        Item::new(*name, ItemKind::Predicate, format!("predicate: {desc}"))
                            .insert(insert, Some(name.len() + 1)),
                    );
                }
                for (name, full) in ARG_TOKENS {
                    items.push(Item::new(*name, ItemKind::Enum, *full));
                }
                items.push(Item::new("__", ItemKind::Keyword, "anonymous traversal"));
                for class in [
                    "P",
                    "TextP",
                    "T",
                    "Order",
                    "Scope",
                    "Column",
                    "Pop",
                    "Cardinality",
                ] {
                    items.push(Item::new(class, ItemKind::Keyword, "enum class"));
                }
                items.extend(steps_after(""));
            }
            rank(items, &typed, |i| i.label.clone())
        }
        Tail::Other => {
            if typed.is_empty() {
                Vec::new()
            } else {
                let mut items = vec![
                    Item::new("g", ItemKind::Keyword, "the graph traversal source").boost(3),
                    Item::new("__", ItemKind::Keyword, "anonymous traversal"),
                ];
                items.extend(snippet_items());
                rank(items, &typed, |i| i.label.clone())
            }
        }
    };
    Completions { replace, items }
}

fn enum_items(class: &str, members: &[(&str, &str)]) -> Vec<Item> {
    members
        .iter()
        .map(|(name, desc)| {
            let detail = format!("{class}.{name}: {desc}");
            if matches!(class, "P" | "TextP") {
                Item::new(*name, ItemKind::Predicate, detail)
                    .insert(format!("{name}()"), Some(name.len() + 1))
            } else {
                Item::new(*name, ItemKind::Enum, detail)
            }
        })
        .collect()
}

/// Steps valid after a step (no source-only steps), plus snippets.
fn steps_after(typed: &str) -> Vec<Item> {
    let catalogue = Catalogue::get();
    let mut items: Vec<Item> = catalogue
        .steps()
        .iter()
        .filter(|s| !SOURCE_ONLY.contains(&s.name.as_str()))
        .filter_map(|s| step_item(&s.name, true))
        .collect();
    // Terminal steps last.
    for item in &mut items {
        if catalogue
            .step(&item.label)
            .is_some_and(|s| s.kind == StepKind::Terminal)
        {
            item.boost = 0;
        } else if item.boost == 0 {
            item.boost = 1;
        }
    }
    items.extend(snippet_items());
    rank(items, typed, |i| i.label.clone())
}

// ---- signature help ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub label: String,
    /// Byte ranges of each parameter within `label`.
    pub params: Vec<Range<usize>>,
    pub variadic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureHelp {
    pub step: String,
    pub signatures: Vec<Signature>,
    pub active: usize,
    /// Index into `signatures[active].params` (clamped to the last for varargs).
    pub active_param: usize,
}

impl SignatureHelp {
    pub fn active_signature(&self) -> &Signature {
        &self.signatures[self.active]
    }

    /// The active parameter's text (`String propertyKey`).
    pub fn active_param_text(&self) -> Option<&str> {
        let sig = self.active_signature();
        sig.params
            .get(self.active_param)
            .map(|r| &sig.label[r.clone()])
    }
}

fn parse_signature(sig: &str) -> Signature {
    let open = sig.find('(').unwrap_or(sig.len());
    let close = sig.rfind(')').unwrap_or(sig.len());
    let mut params = Vec::new();
    let inner = &sig[(open + 1).min(close)..close];
    let mut start = open + 1;
    if !inner.trim().is_empty() {
        for part in inner.split(", ") {
            params.push(start..start + part.len());
            start += part.len() + 2;
        }
    }
    let variadic = inner.contains("...");
    Signature {
        label: sig.to_owned(),
        params,
        variadic,
    }
}

fn param_type(sig: &Signature, i: usize) -> &str {
    sig.params
        .get(i)
        .map(|r| sig.label[r.clone()].split(' ').next().unwrap_or(""))
        .unwrap_or("")
}

/// Does argument `arg` plausibly fit a parameter of type `ty`?
fn fits(ty: &str, arg: &Arg) -> bool {
    let raw = arg.raw.trim();
    let ty = ty.trim_end_matches("...");
    if arg.string.is_some() {
        return matches!(ty, "String" | "Object");
    }
    let looks_predicate = raw.starts_with("P.")
        || raw.starts_with("TextP.")
        || ENUMS[0]
            .1
            .iter()
            .chain(ENUMS[1].1)
            .any(|(p, _)| raw.starts_with(&format!("{p}(")));
    if looks_predicate {
        return matches!(ty, "P" | "Object");
    }
    if raw.starts_with("T.") || matches!(raw, "id" | "label") {
        return matches!(ty, "T" | "Object");
    }
    if raw.starts_with("__") || raw.contains('(') {
        return matches!(ty, "Traversal" | "Object");
    }
    true
}

/// Signature help when `offset` is inside a step's parentheses.
pub fn signature_help(text: &str, offset: usize, schema: Option<&Schema>) -> Option<SignatureHelp> {
    let offset = offset.min(text.len());
    if !text.is_char_boundary(offset) {
        return None;
    }
    let scanned = scan(&text[..offset], false);
    let level = scanned.levels.last()?;
    let info = Catalogue::get().step(&level.call.name)?;
    let args = &level.call.args;
    let index = args.len();
    let signatures: Vec<Signature> = info.signatures.iter().map(|s| parse_signature(s)).collect();
    let mut best: Option<(usize, i32)> = None;
    for (i, sig) in signatures.iter().enumerate() {
        let arity_ok = sig.params.len() > index || (sig.variadic && !sig.params.is_empty());
        if !arity_ok {
            continue;
        }
        let mut score = 0;
        for (j, arg) in args.iter().enumerate() {
            let ty = param_type(sig, j.min(sig.params.len().saturating_sub(1)));
            if fits(ty, arg) {
                score += 1;
            } else {
                score -= 10;
            }
        }
        // `has('airport', …)` with a known label: the label form.
        if info.name == "has" && index >= 1 {
            let named_label = sig.label.contains("String label");
            if let Some(first) = args.first().and_then(|a| a.string.as_deref()) {
                let is_label = !is_key_anywhere(schema, first) && is_label_anywhere(schema, first);
                if named_label && is_label {
                    score += 5;
                }
            }
        }
        if best.is_none_or(|(_, s)| score > s) {
            best = Some((i, score));
        }
    }
    let active = best.map_or(0, |(i, _)| i);
    let sig = &signatures[active];
    let active_param = index.min(sig.params.len().saturating_sub(1));
    Some(SignatureHelp {
        step: info.name.clone(),
        signatures,
        active,
        active_param,
    })
}

// ---- schema warnings --------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaIssue {
    /// The string's content (inside the quotes).
    pub range: Range<usize>,
    pub message: String,
    /// The closest known name, for a quick fix.
    pub replacement: Option<String>,
}

/// Levenshtein distance (small strings).
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// The closest of `candidates` within distance 2.
fn closest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<String> {
    candidates
        .into_iter()
        .map(|c| (edit_distance(name, c), c))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c.to_owned())
}

/// Labels and property keys in `text` that the schema does not know. Parts the schema marks partial
/// (and schemas still being discovered) are not checked.
pub fn schema_issues(text: &str, schema: &Schema) -> Vec<SchemaIssue> {
    // A schema still being discovered would flag names it simply has not reached yet.
    if schema.in_progress {
        return Vec::new();
    }
    let scanned = scan(text, true);
    let mut out = Vec::new();
    for site in &scanned.sites {
        let Some(level) = site.levels.last() else {
            continue;
        };
        let call = &level.call;
        let flow = flow::at_args(&site.levels, Some(schema));
        for (i, arg) in call.args.iter().enumerate() {
            let Some(value) = &arg.string else { continue };
            // With all arguments known, `has(a, b, c)` starts with a label.
            let role = if call.name == "has" && call.args.len() == 3 && i == 0 {
                Role::Label
            } else {
                role(&call.name, i, &call.args[..i], Some(schema))
            };
            let issue = match role {
                Role::Label | Role::EdgeLabel if !schema.partial.labels => {
                    let edges = matches!(role, Role::EdgeLabel) || flow.is_edges();
                    let (known, kind): (Vec<&str>, &str) = if edges {
                        (
                            schema.edge_labels.iter().map(|l| l.name.as_str()).collect(),
                            "edge label",
                        )
                    } else {
                        (
                            schema
                                .vertex_labels
                                .iter()
                                .map(|l| l.name.as_str())
                                .collect(),
                            "vertex label",
                        )
                    };
                    (!known.contains(&value.as_str())).then(|| (kind, closest(value, known)))
                }
                // `property()` creates keys; only reads are checked.
                Role::PropertyKey { .. }
                    if !schema.partial.properties && call.name != "property" =>
                {
                    let preferred: Vec<String> = keys_for(schema, &flow, None)
                        .into_iter()
                        .map(|k| k.0)
                        .collect();
                    let all: Vec<String> = keys_for(schema, &Flow::Other, None)
                        .into_iter()
                        .map(|k| k.0)
                        .collect();
                    (!all.contains(value)).then(|| {
                        let fix = closest(value, preferred.iter().map(String::as_str))
                            .or_else(|| closest(value, all.iter().map(String::as_str)));
                        ("property key", fix)
                    })
                }
                _ => None,
            };
            if let Some((kind, replacement)) = issue {
                let message = match &replacement {
                    Some(fix) => format!("Unknown {kind} `{value}`. Did you mean `{fix}`?"),
                    None => format!("Unknown {kind} `{value}` (not in the discovered schema)"),
                };
                out.push(SchemaIssue {
                    range: arg.span.clone(),
                    message,
                    replacement,
                });
            }
        }
    }
    out
}

/// Edge steps whose label never connects the labels at that point in that direction (e.g.
/// `hasLabel('airport').out('contains')`, when `contains` points into airports).
pub fn direction_issues(text: &str, schema: &Schema) -> Vec<SchemaIssue> {
    if schema.in_progress || schema.partial.triples {
        return Vec::new();
    }
    let scanned = scan(text, true);
    let mut out = Vec::new();
    for site in &scanned.sites {
        let Some(level) = site.levels.last() else {
            continue;
        };
        let call = &level.call;
        let dir = match call.name.as_str() {
            "out" | "outE" => flow::Dir::Out,
            "in" | "inE" => flow::Dir::In,
            _ => continue,
        };
        let flow = flow::at_args(&site.levels, Some(schema));
        let Some(from) = flow.vertex_labels() else {
            continue;
        };
        for arg in &call.args {
            let Some(label) = &arg.string else { continue };
            let triples: Vec<_> = schema
                .triples
                .iter()
                .filter(|t| &t.label == label)
                .collect();
            if triples.is_empty() {
                continue; // unknown labels are reported by schema_issues
            }
            let forward = triples.iter().any(|t| match dir {
                flow::Dir::Out => from.contains(&t.out_label),
                _ => from.contains(&t.in_label),
            });
            if forward {
                continue;
            }
            let reverse = triples.iter().any(|t| match dir {
                flow::Dir::Out => from.contains(&t.in_label),
                _ => from.contains(&t.out_label),
            });
            let from_text = from.iter().cloned().collect::<Vec<_>>().join(", ");
            let (message, replacement) = if reverse {
                let other = match dir {
                    flow::Dir::Out => call.name.replacen("out", "in", 1),
                    _ => call.name.replacen("in", "out", 1),
                };
                (
                    format!(
                        "`{label}` edges point the other way for `{from_text}`: use {other}('{label}') instead of {}('{label}')",
                        call.name
                    ),
                    Some(other),
                )
            } else {
                (
                    format!("No `{label}` edges touch `{from_text}` in the schema"),
                    None,
                )
            };
            out.push(SchemaIssue {
                range: call.name_span.clone(),
                message,
                replacement,
            });
        }
    }
    out
}

/// Groovy closures (`{ it -> … }`) in `text`, by byte range.
pub fn lambdas(text: &str) -> Vec<Range<usize>> {
    scan::tokenize(text)
        .into_iter()
        .filter(|t| t.kind == scan::Tok::Other && text[t.span.clone()].starts_with('{'))
        .map(|t| t.span)
        .collect()
}

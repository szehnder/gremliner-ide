//! The Graph Explorer's model (graph-explorer): a linear path of vertex and edge steps with typed
//! property filters, turned into an idiomatic traversal with its values as bindings (ADR-0030).

use gremlin_model::GValue;
use schema::Schema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Out,
    In,
    Both,
}

impl Direction {
    pub const ALL: [Direction; 3] = [Direction::Out, Direction::In, Direction::Both];

    pub fn id(self) -> &'static str {
        match self {
            Direction::Out => "out",
            Direction::In => "in",
            Direction::Both => "both",
        }
    }
}

/// What a property holds, from the schema's sampled types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    Integer,
    Decimal,
    Text,
    Boolean,
    Date,
}

impl ValueType {
    /// From GraphSON type names, most common first (`["Int32"]`). Unknown types are treated as text.
    pub fn from_types(types: &[String]) -> Self {
        match types.first().map(String::as_str) {
            Some("Int32" | "Int64" | "Short" | "Byte" | "BigInteger") => ValueType::Integer,
            Some("Double" | "Float" | "BigDecimal") => ValueType::Decimal,
            Some("Boolean") => ValueType::Boolean,
            Some("Date" | "Timestamp" | "OffsetDateTime") => ValueType::Date,
            _ => ValueType::Text,
        }
    }

    pub fn is_ordered(self) -> bool {
        matches!(
            self,
            ValueType::Integer | ValueType::Decimal | ValueType::Date
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    Between,
    Inside,
    Outside,
    Within,
    Without,
    StartingWith,
    EndingWith,
    Containing,
    NotContaining,
}

const ORDERED_OPS: &[Op] = &[
    Op::Eq,
    Op::Neq,
    Op::Gt,
    Op::Gte,
    Op::Lt,
    Op::Lte,
    Op::Between,
    Op::Inside,
    Op::Outside,
    Op::Within,
    Op::Without,
];
const TEXT_OPS: &[Op] = &[
    Op::Eq,
    Op::Neq,
    Op::StartingWith,
    Op::EndingWith,
    Op::Containing,
    Op::NotContaining,
    Op::Within,
    Op::Without,
];
const BOOL_OPS: &[Op] = &[Op::Eq, Op::Neq];

impl Op {
    /// The operators offered for a property of type `t`.
    pub fn for_type(t: ValueType) -> &'static [Op] {
        match t {
            ValueType::Integer | ValueType::Decimal | ValueType::Date => ORDERED_OPS,
            ValueType::Text => TEXT_OPS,
            ValueType::Boolean => BOOL_OPS,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Op::Eq => "equals",
            Op::Neq => "not equals",
            Op::Gt => "greater than",
            Op::Gte => "greater than or equal",
            Op::Lt => "less than",
            Op::Lte => "less than or equal",
            Op::Between => "between (lower inclusive)",
            Op::Inside => "inside (exclusive)",
            Op::Outside => "outside",
            Op::Within => "is one of",
            Op::Without => "is none of",
            Op::StartingWith => "starting with",
            Op::EndingWith => "ending with",
            Op::Containing => "containing",
            Op::NotContaining => "not containing",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Op::Eq => "eq",
            Op::Neq => "neq",
            Op::Gt => "gt",
            Op::Gte => "gte",
            Op::Lt => "lt",
            Op::Lte => "lte",
            Op::Between => "between",
            Op::Inside => "inside",
            Op::Outside => "outside",
            Op::Within => "within",
            Op::Without => "without",
            Op::StartingWith => "startingWith",
            Op::EndingWith => "endingWith",
            Op::Containing => "containing",
            Op::NotContaining => "notContaining",
        }
    }

    /// Values the operator takes: 1, 2 (a range), or a list (`None`).
    pub fn arity(self) -> Option<usize> {
        match self {
            Op::Between | Op::Inside | Op::Outside => Some(2),
            Op::Within | Op::Without => None,
            _ => Some(1),
        }
    }

    /// The input hint for the value field.
    pub fn placeholder(self) -> &'static str {
        match self.arity() {
            Some(2) => "lower, upper",
            None => "a, b, c",
            _ => "value",
        }
    }

    fn predicate(self, args: &str) -> String {
        match self {
            Op::Eq => args.to_owned(),
            Op::StartingWith | Op::EndingWith | Op::Containing | Op::NotContaining => {
                format!("TextP.{}({args})", self.id())
            }
            other => format!("P.{}({args})", other.id()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub key: String,
    pub op: Op,
    /// As typed: one value, `lower, upper`, or a comma-separated list.
    pub value: String,
}

// A path has a handful of steps; boxing the fixed start would only complicate matching.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Vertex {
        /// `None` is "any label".
        label: Option<String>,
        filters: Vec<Filter>,
        /// Start from this vertex ("Search path from here"), with a name to show.
        fixed: Option<(GValue, String)>,
    },
    Edge {
        label: Option<String>,
        direction: Direction,
        filters: Vec<Filter>,
    },
}

impl Step {
    pub fn vertex(label: Option<&str>) -> Self {
        Step::Vertex {
            label: label.map(str::to_owned),
            filters: Vec::new(),
            fixed: None,
        }
    }

    pub fn edge(label: Option<&str>, direction: Direction) -> Self {
        Step::Edge {
            label: label.map(str::to_owned),
            direction,
            filters: Vec::new(),
        }
    }

    pub fn is_vertex(&self) -> bool {
        matches!(self, Step::Vertex { .. })
    }

    pub fn label(&self) -> Option<&str> {
        match self {
            Step::Vertex { label, .. } | Step::Edge { label, .. } => label.as_deref(),
        }
    }

    pub fn filters(&self) -> &[Filter] {
        match self {
            Step::Vertex { filters, .. } | Step::Edge { filters, .. } => filters,
        }
    }

    pub fn filters_mut(&mut self) -> &mut Vec<Filter> {
        match self {
            Step::Vertex { filters, .. } | Step::Edge { filters, .. } => filters,
        }
    }

    pub fn set_label(&mut self, l: Option<String>) {
        match self {
            Step::Vertex { label, .. } | Step::Edge { label, .. } => *label = l,
        }
    }
}

pub const DEFAULT_LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq)]
pub struct ExplorerPath {
    pub steps: Vec<Step>,
    pub limit: usize,
    pub randomize: bool,
}

impl Default for ExplorerPath {
    fn default() -> Self {
        Self {
            steps: vec![Step::vertex(None)],
            limit: DEFAULT_LIMIT,
            randomize: false,
        }
    }
}

/// A traversal with its values as bindings `p0, p1, ...`.
#[derive(Debug, Clone, PartialEq)]
pub struct Generated {
    pub script: String,
    pub bindings: Vec<(String, GValue)>,
}

impl Generated {
    /// The script with each binding replaced by its literal, ready to run in an editor tab.
    pub fn inlined(&self) -> String {
        // Replace longest names first so `p10` is not hit by `p1`.
        let mut names: Vec<&(String, GValue)> = self.bindings.iter().collect();
        names.sort_by_key(|(n, _)| std::cmp::Reverse(n.len()));
        let mut out = self.script.clone();
        for (name, value) in names {
            out = replace_ident(&out, name, &literal(value));
        }
        out
    }
}

/// Replace whole-word occurrences of `name` outside quotes.
fn replace_ident(text: &str, name: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            out.push(c as char);
            if c == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == b'\'' || c == b'"' {
            quote = Some(c);
            out.push(c as char);
            i += 1;
            continue;
        }
        let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        if text[i..].starts_with(name)
            && (i == 0 || !word(bytes[i - 1]))
            && bytes.get(i + name.len()).is_none_or(|&b| !word(b))
        {
            out.push_str(with);
            i += name.len();
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

/// A Groovy literal for a bound value.
pub fn literal(v: &GValue) -> String {
    match v {
        GValue::String(s) => {
            let escaped = s.replace('\\', "\\\\").replace('\'', "\\'");
            format!("'{escaped}'")
        }
        GValue::Int64(n) => format!("{n}L"),
        GValue::Int32(n) => n.to_string(),
        GValue::Double(d) => format!("{d}d"),
        GValue::Bool(b) => b.to_string(),
        GValue::Date(ms) => format!("new Date({ms}L)"),
        GValue::List(items) => format!(
            "[{}]",
            items.iter().map(literal).collect::<Vec<_>>().join(", ")
        ),
        other => gremlin_model::console::format(other),
    }
}

fn quote(s: &str) -> String {
    literal(&GValue::String(s.to_owned()))
}

/// `YYYY-MM-DD` → milliseconds since the epoch (UTC midnight).
fn parse_date(s: &str) -> Option<i64> {
    let mut parts = s.trim().splitn(3, '-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.get(..2)?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from civil (Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400_000)
}

/// Parse one typed value.
pub fn parse_value(text: &str, t: ValueType) -> Result<GValue, String> {
    let s = text.trim();
    if s.is_empty() {
        return Err("Enter a value.".into());
    }
    match t {
        ValueType::Integer => s
            .replace(',', "")
            .parse::<i64>()
            .map(GValue::Int64)
            .map_err(|_| format!("“{s}” is not a whole number.")),
        ValueType::Decimal => s
            .parse::<f64>()
            .map(GValue::Double)
            .map_err(|_| format!("“{s}” is not a number.")),
        ValueType::Boolean => match s.to_ascii_lowercase().as_str() {
            "true" | "yes" => Ok(GValue::Bool(true)),
            "false" | "no" => Ok(GValue::Bool(false)),
            _ => Err(format!("“{s}” is not true or false.")),
        },
        ValueType::Date => parse_date(s)
            .map(GValue::Date)
            .ok_or_else(|| format!("“{s}” is not a date (YYYY-MM-DD).")),
        ValueType::Text => Ok(GValue::String(s.to_owned())),
    }
}

/// The type of `key` on the step's label (any label: the first label that has the key).
pub fn property_type(schema: Option<&Schema>, step: &Step, key: &str) -> ValueType {
    let Some(schema) = schema else {
        return ValueType::Text;
    };
    let labels = if step.is_vertex() {
        &schema.vertex_labels
    } else {
        &schema.edge_labels
    };
    labels
        .iter()
        .filter(|l| step.label().is_none_or(|want| l.name == want))
        .find_map(|l| l.properties.iter().find(|p| p.key == key))
        .map(|p| ValueType::from_types(&p.types))
        .unwrap_or(ValueType::Text)
}

/// Property keys a step can filter on, from the schema.
pub fn property_keys(schema: Option<&Schema>, step: &Step) -> Vec<String> {
    let Some(schema) = schema else {
        return Vec::new();
    };
    let labels = if step.is_vertex() {
        &schema.vertex_labels
    } else {
        &schema.edge_labels
    };
    let mut keys: Vec<String> = labels
        .iter()
        .filter(|l| step.label().is_none_or(|want| l.name == want))
        .flat_map(|l| l.properties.iter().map(|p| p.key.clone()))
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

impl ExplorerPath {
    /// Why the path cannot run, if it cannot.
    pub fn problem(&self) -> Option<&'static str> {
        match self.steps.last() {
            None => Some("Add a vertex step to start the path."),
            Some(Step::Edge { .. }) => Some("Add a vertex step: a path must end on a vertex."),
            _ if self.limit == 0 => Some("Set a limit of at least 1."),
            _ => None,
        }
    }

    /// A hint for searches likely to explode.
    pub fn warning(&self) -> Option<&'static str> {
        let any = self
            .steps
            .iter()
            .filter(|s| s.label().is_none() && s.filters().is_empty())
            .count();
        (any > 1)
            .then_some("Several steps match anything; the search may be slow on a large graph.")
    }

    /// Add an edge after the last vertex, or a vertex after the last edge.
    pub fn push(&mut self) {
        match self.steps.last() {
            Some(Step::Vertex { .. }) => self.steps.push(Step::edge(None, Direction::Out)),
            _ => self.steps.push(Step::vertex(None)),
        }
    }

    pub fn pop(&mut self) {
        if self.steps.len() > 1 {
            self.steps.pop();
        }
    }

    /// The traversal, or the first filter value that does not parse.
    pub fn generate(&self, schema: Option<&Schema>) -> Result<Generated, String> {
        if let Some(problem) = self.problem() {
            return Err(problem.into());
        }
        let mut bindings: Vec<(String, GValue)> = Vec::new();
        let mut bind = |v: GValue| {
            let name = format!("p{}", bindings.len());
            bindings.push((name.clone(), v));
            name
        };
        let mut script = String::from("g");
        let filter_calls = |step: &Step,
                            skip_first_with_label: bool,
                            bind: &mut dyn FnMut(GValue) -> String|
         -> Result<Vec<String>, String> {
            let mut calls = Vec::new();
            for (n, f) in step.filters().iter().enumerate() {
                if f.key.trim().is_empty() {
                    continue;
                }
                let t = property_type(schema, step, &f.key);
                let values: Vec<&str> = match f.op.arity() {
                    Some(1) => vec![f.value.as_str()],
                    _ => f.value.split(',').collect(),
                };
                if f.op.arity() == Some(2) && values.len() != 2 {
                    return Err(format!(
                        "{} {} needs two values: lower, upper.",
                        f.key,
                        f.op.label()
                    ));
                }
                let parsed: Vec<GValue> = values
                    .iter()
                    .map(|v| parse_value(v, t).map_err(|e| format!("{}: {e}", f.key)))
                    .collect::<Result<_, _>>()?;
                let args = match f.op.arity() {
                    None => bind(GValue::List(parsed)),
                    _ => parsed
                        .into_iter()
                        .map(&mut *bind)
                        .collect::<Vec<_>>()
                        .join(", "),
                };
                let pred = f.op.predicate(&args);
                // The first filter carries the label: has(label, key, predicate) lets indexes help.
                let call = match (n, skip_first_with_label, step.label()) {
                    (0, true, Some(label)) => {
                        format!("has({}, {}, {pred})", quote(label), quote(&f.key))
                    }
                    _ => format!("has({}, {pred})", quote(&f.key)),
                };
                calls.push(call);
            }
            Ok(calls)
        };
        for (i, step) in self.steps.iter().enumerate() {
            match step {
                Step::Vertex { label, fixed, .. } => {
                    if i == 0 {
                        match fixed {
                            Some((id, _)) => script.push_str(&format!(".V({})", bind(id.clone()))),
                            None => script.push_str(".V()"),
                        }
                    }
                    let calls = filter_calls(step, true, &mut bind)?;
                    if calls.is_empty()
                        && let Some(l) = label
                    {
                        script.push_str(&format!(".hasLabel({})", quote(l)));
                    }
                    for c in calls {
                        script.push('.');
                        script.push_str(&c);
                    }
                }
                Step::Edge {
                    label, direction, ..
                } => {
                    let (edge, vertex) = match direction {
                        Direction::Out => ("outE", "inV"),
                        Direction::In => ("inE", "outV"),
                        Direction::Both => ("bothE", "otherV"),
                    };
                    match label {
                        Some(l) => script.push_str(&format!(".{edge}({})", quote(l))),
                        None => script.push_str(&format!(".{edge}()")),
                    }
                    for c in filter_calls(step, false, &mut bind)? {
                        script.push('.');
                        script.push_str(&c);
                    }
                    script.push_str(&format!(".{vertex}()"));
                }
            }
        }
        script.push_str(".path()");
        if self.randomize {
            script.push_str(".order().by(shuffle)");
        }
        script.push_str(&format!(".limit({})", self.limit));
        Ok(Generated { script, bindings })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn filter(key: &str, op: Op, value: &str) -> Filter {
        Filter {
            key: key.into(),
            op,
            value: value.into(),
        }
    }

    #[test]
    fn generation_with_bindings() {
        let mut path = ExplorerPath {
            steps: vec![
                Step::vertex(Some("airport")),
                Step::edge(Some("route"), Direction::Out),
                Step::vertex(None),
            ],
            limit: 50,
            randomize: false,
        };
        path.steps[0]
            .filters_mut()
            .push(filter("city", Op::StartingWith, "San"));
        path.steps[1]
            .filters_mut()
            .push(filter("dist", Op::Gt, "1000"));
        let g = path.generate(None).unwrap();
        assert_eq!(
            g.script,
            "g.V().has('airport', 'city', TextP.startingWith(p0)).outE('route').has('dist', P.gt(p1)).inV().path().limit(50)"
        );
        assert_eq!(
            g.bindings,
            vec![
                ("p0".to_owned(), GValue::String("San".into())),
                ("p1".to_owned(), GValue::String("1000".into()))
            ]
        );
        assert_eq!(
            g.inlined(),
            "g.V().has('airport', 'city', TextP.startingWith('San')).outE('route').has('dist', P.gt('1000')).inV().path().limit(50)"
        );
    }

    #[test]
    fn values_parse_by_type() {
        assert_eq!(parse_value("3", ValueType::Integer), Ok(GValue::Int64(3)));
        assert!(parse_value("x", ValueType::Integer).is_err());
        assert_eq!(
            parse_value("2020-01-02", ValueType::Date),
            Ok(GValue::Date(1_577_923_200_000))
        );
        assert_eq!(
            parse_value("Yes", ValueType::Boolean),
            Ok(GValue::Bool(true))
        );
        assert_eq!(
            literal(&GValue::List(vec![
                GValue::Int64(1),
                GValue::String("a'b".into())
            ])),
            "[1L, 'a\\'b']"
        );
    }

    #[test]
    fn inlining_leaves_strings_and_longer_names_alone() {
        let g = Generated {
            script: "has('p1', p1).has(p10)".into(),
            bindings: (0..11)
                .map(|i| (format!("p{i}"), GValue::Int32(i)))
                .collect(),
        };
        assert_eq!(g.inlined(), "has('p1', 1).has(10)");
    }
}

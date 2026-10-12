//! The discovered schema: labels with counts and typed properties, plus edge connectivity triples. It is
//! plain data (serialised to the per-connection cache as JSON) and knows nothing about how it was found.

use serde::{Deserialize, Serialize};

/// Bumped when the JSON shape changes incompatibly; older cache entries are discarded.
pub const SCHEMA_VERSION: u32 = 1;

/// Longest sample value kept, in characters (longer values end in `…`).
pub const SAMPLE_MAX_CHARS: usize = 64;

/// Distinct sample values kept per property.
pub const SAMPLES_PER_PROPERTY: usize = 5;

/// Where the labels and property keys came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Sampled Gremlin queries.
    #[default]
    Sampled,
    /// The database's statistics summary (Neptune), with sampling only for types, samples and triples.
    Summary,
}

/// An element count, exact or capped (`≥ value`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Count {
    pub value: u64,
    /// The counting query stopped at its cap; the real count is at least `value`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub capped: bool,
}

impl Count {
    pub fn exact(value: u64) -> Self {
        Self {
            value,
            capped: false,
        }
    }

    /// `3,504` or `≥ 100,000`.
    pub fn text(&self) -> String {
        let digits = self.value.to_string();
        let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                grouped.push(',');
            }
            grouped.push(c);
        }
        if self.capped {
            format!("\u{2265} {grouped}")
        } else {
            grouped
        }
    }
}

/// Whether an element was seen holding several values for one key (list/set vertex properties).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    #[default]
    Single,
    Multi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyInfo {
    pub key: String,
    /// GraphSON type names seen in samples, most common first (`["Int32"]`, `["Int32", "String"]`).
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub cardinality: Cardinality,
    /// Up to [`SAMPLES_PER_PROPERTY`] distinct values as text, each at most [`SAMPLE_MAX_CHARS`] characters.
    #[serde(default)]
    pub samples: Vec<String>,
}

impl PropertyInfo {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            types: Vec::new(),
            cardinality: Cardinality::Single,
            samples: Vec::new(),
        }
    }

    /// `String`, `Int32 | String`, or `?` when no value was sampled.
    pub fn type_text(&self) -> String {
        if self.types.is_empty() {
            "?".into()
        } else {
            self.types.join(" | ")
        }
    }

    /// The majority type, if any value was sampled.
    pub fn primary_type(&self) -> Option<&str> {
        self.types.first().map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<Count>,
    #[serde(default)]
    pub properties: Vec<PropertyInfo>,
}

impl LabelInfo {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            count: None,
            properties: Vec::new(),
        }
    }

    pub fn property(&self, key: &str) -> Option<&PropertyInfo> {
        self.properties.iter().find(|p| p.key == key)
    }

    pub(crate) fn property_mut(&mut self, key: &str) -> &mut PropertyInfo {
        let i = match self.properties.iter().position(|p| p.key == key) {
            Some(i) => i,
            None => {
                self.properties.push(PropertyInfo::new(key));
                self.properties.len() - 1
            }
        };
        &mut self.properties[i]
    }
}

/// `(out_label)-[label]->(in_label)`: an edge label seen connecting two vertex labels.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Triple {
    #[serde(rename = "out")]
    pub out_label: String,
    pub label: String,
    #[serde(rename = "in")]
    pub in_label: String,
}

impl Triple {
    pub fn new(out_label: &str, label: &str, in_label: &str) -> Self {
        Self {
            out_label: out_label.into(),
            label: label.into(),
            in_label: in_label.into(),
        }
    }

    /// `person-knows->person`.
    pub fn text(&self) -> String {
        format!("{}-{}->{}", self.out_label, self.label, self.in_label)
    }
}

/// Which parts hit a sample limit or timeout and may be incomplete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Partial {
    #[serde(default)]
    pub labels: bool,
    #[serde(default)]
    pub properties: bool,
    #[serde(default)]
    pub counts: bool,
    #[serde(default)]
    pub triples: bool,
}

impl Partial {
    pub fn any(&self) -> bool {
        self.labels || self.properties || self.counts || self.triples
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Schema {
    #[serde(default = "current_version")]
    pub schema_version: u32,
    /// Milliseconds since the Unix epoch when discovery finished (or the snapshot was taken).
    pub discovered_at_ms: i64,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub partial: Partial,
    /// Discovery is still running; later snapshots will add to this one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub in_progress: bool,
    pub vertex_labels: Vec<LabelInfo>,
    pub edge_labels: Vec<LabelInfo>,
    pub triples: Vec<Triple>,
    /// Problems worth showing (failed queries), without failing discovery.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

fn current_version() -> u32 {
    SCHEMA_VERSION
}

impl Schema {
    pub fn new(discovered_at_ms: i64) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            discovered_at_ms,
            ..Self::default()
        }
    }

    pub fn vertex_label(&self, name: &str) -> Option<&LabelInfo> {
        self.vertex_labels.iter().find(|l| l.name == name)
    }

    pub fn edge_label(&self, name: &str) -> Option<&LabelInfo> {
        self.edge_labels.iter().find(|l| l.name == name)
    }

    pub(crate) fn vertex_label_mut(&mut self, name: &str) -> &mut LabelInfo {
        label_mut(&mut self.vertex_labels, name)
    }

    pub(crate) fn edge_label_mut(&mut self, name: &str) -> &mut LabelInfo {
        label_mut(&mut self.edge_labels, name)
    }

    /// True when the label is known as a vertex or an edge label.
    pub fn knows_label(&self, name: &str) -> bool {
        self.vertex_label(name).is_some() || self.edge_label(name).is_some()
    }

    /// Labels in `seen` that this schema does not know (results showing them suggest a refresh).
    pub fn unknown_labels<'a>(&self, seen: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let mut out: Vec<String> = seen
            .into_iter()
            .filter(|l| !self.knows_label(l))
            .map(str::to_owned)
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Sort labels, properties and triples by name so snapshots are stable.
    pub fn normalize(&mut self) {
        for list in [&mut self.vertex_labels, &mut self.edge_labels] {
            list.sort_by(|a, b| a.name.cmp(&b.name));
            for label in list.iter_mut() {
                label.properties.sort_by(|a, b| a.key.cmp(&b.key));
            }
        }
        self.triples.sort();
        self.triples.dedup();
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Parse a cached schema; `None` for malformed JSON or another schema version.
    pub fn from_json(text: &str) -> Option<Self> {
        let schema: Self = serde_json::from_str(text).ok()?;
        (schema.schema_version == SCHEMA_VERSION).then_some(schema)
    }

    /// Human-readable text for the CLI.
    pub fn render_text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let section = |out: &mut String, title: &str, labels: &[LabelInfo]| {
            let _ = writeln!(out, "{title}:");
            if labels.is_empty() {
                let _ = writeln!(out, "  (none)");
            }
            for label in labels {
                let count = label
                    .count
                    .map(|c| format!(" ({})", c.text()))
                    .unwrap_or_default();
                let _ = writeln!(out, "  {}{count}", label.name);
                for p in &label.properties {
                    let multi = if p.cardinality == Cardinality::Multi {
                        " [multi]"
                    } else {
                        ""
                    };
                    let samples = if p.samples.is_empty() {
                        String::new()
                    } else {
                        format!("  e.g. {}", p.samples.join(", "))
                    };
                    let _ = writeln!(out, "    {}: {}{multi}{samples}", p.key, p.type_text());
                }
            }
        };
        section(&mut out, "Vertex labels", &self.vertex_labels);
        section(&mut out, "Edge labels", &self.edge_labels);
        let _ = writeln!(out, "Connections:");
        if self.triples.is_empty() {
            let _ = writeln!(out, "  (none)");
        }
        for t in &self.triples {
            let _ = writeln!(out, "  ({})-[{}]->({})", t.out_label, t.label, t.in_label);
        }
        if self.partial.any() {
            let mut parts = Vec::new();
            if self.partial.labels {
                parts.push("labels");
            }
            if self.partial.properties {
                parts.push("properties");
            }
            if self.partial.counts {
                parts.push("counts");
            }
            if self.partial.triples {
                parts.push("connections");
            }
            let _ = writeln!(
                out,
                "Partial: {} (a sample limit or timeout was reached)",
                parts.join(", ")
            );
        }
        for note in &self.notes {
            let _ = writeln!(out, "Note: {note}");
        }
        out
    }
}

fn label_mut<'a>(list: &'a mut Vec<LabelInfo>, name: &str) -> &'a mut LabelInfo {
    let i = match list.iter().position(|l| l.name == name) {
        Some(i) => i,
        None => {
            list.push(LabelInfo::new(name));
            list.len() - 1
        }
    };
    &mut list[i]
}

/// Truncate a sample to [`SAMPLE_MAX_CHARS`] characters, ending in `…` when cut.
pub fn truncate_sample(text: &str) -> String {
    if text.chars().count() <= SAMPLE_MAX_CHARS {
        text.to_owned()
    } else {
        let mut cut: String = text.chars().take(SAMPLE_MAX_CHARS - 1).collect();
        cut.push('\u{2026}');
        cut
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_text_groups_digits() {
        assert_eq!(Count::exact(7).text(), "7");
        assert_eq!(Count::exact(3504).text(), "3,504");
        assert_eq!(
            Count {
                value: 100_000,
                capped: true
            }
            .text(),
            "\u{2265} 100,000"
        );
    }

    #[test]
    fn truncates_long_samples() {
        let long = "x".repeat(100);
        let cut = truncate_sample(&long);
        assert_eq!(cut.chars().count(), SAMPLE_MAX_CHARS);
        assert!(cut.ends_with('\u{2026}'));
        assert_eq!(truncate_sample("AUS"), "AUS");
    }

    #[test]
    fn json_round_trip_and_version_check() {
        let mut s = Schema::new(5);
        s.vertex_label_mut("person").property_mut("name").types = vec!["String".into()];
        s.triples.push(Triple::new("person", "knows", "person"));
        let back = Schema::from_json(&s.to_json()).expect("parses");
        assert_eq!(back, s);
        let old = s
            .to_json()
            .replace("\"schema_version\":1", "\"schema_version\":0");
        assert!(Schema::from_json(&old).is_none());
    }

    #[test]
    fn unknown_labels_are_sorted_and_unique() {
        let mut s = Schema::new(0);
        s.vertex_label_mut("person");
        s.edge_label_mut("knows");
        assert_eq!(
            s.unknown_labels(["person", "zeta", "knows", "alpha", "zeta"]),
            vec!["alpha".to_owned(), "zeta".to_owned()]
        );
    }
}

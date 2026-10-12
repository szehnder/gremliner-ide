//! A database's statistics summary (Neptune `GET /propertygraph/statistics/summary?mode=detailed`), used
//! instead of sampling for labels, property keys and vertex counts.

use serde_json::Value;

use crate::model::{Count, Schema};

/// What a statistics summary tells us. Edge property keys are not per label in Neptune's summary, so
/// discovery still samples them per edge label.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Summary {
    /// Vertex label, count when known, property keys (empty in `basic` mode).
    pub vertex_labels: Vec<(String, Option<u64>, Vec<String>)>,
    pub edge_labels: Vec<String>,
}

impl Summary {
    /// Parse Neptune's summary response (the `payload` envelope is optional). `None` when it has no
    /// graph summary, e.g. statistics are disabled.
    pub fn from_neptune_json(text: &str) -> Option<Self> {
        let root: Value = serde_json::from_str(text).ok()?;
        let graph = root.get("payload").unwrap_or(&root).get("graphSummary")?;
        let strings = |v: Option<&Value>| -> Vec<String> {
            v.and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut vertex_labels: Vec<(String, Option<u64>, Vec<String>)> =
            strings(graph.get("nodeLabels"))
                .into_iter()
                .map(|l| (l, None, Vec::new()))
                .collect();
        // Detailed mode: structures give counts and property keys per label set.
        for structure in graph
            .get("nodeStructures")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let count = structure.get("count").and_then(Value::as_u64);
            let props = strings(structure.get("nodeProperties"));
            for label in strings(structure.get("nodeLabels")) {
                let entry = match vertex_labels.iter_mut().find(|(l, _, _)| *l == label) {
                    Some(e) => e,
                    None => {
                        vertex_labels.push((label, None, Vec::new()));
                        vertex_labels.last_mut()?
                    }
                };
                if let Some(c) = count {
                    entry.1 = Some(entry.1.unwrap_or(0) + c);
                }
                for p in &props {
                    if !entry.2.contains(p) {
                        entry.2.push(p.clone());
                    }
                }
            }
        }
        Some(Self {
            vertex_labels,
            edge_labels: strings(graph.get("edgeLabels")),
        })
    }

    pub(crate) fn apply(self, schema: &mut Schema) {
        for (name, count, props) in self.vertex_labels {
            let label = schema.vertex_label_mut(&name);
            label.count = count.map(Count::exact);
            for p in props {
                label.property_mut(&p);
            }
        }
        for name in self.edge_labels {
            schema.edge_label_mut(&name);
        }
    }
}

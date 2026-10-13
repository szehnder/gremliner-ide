//! Profile and explain reports, one model for every provider: TinkerPop `TraversalMetrics` and
//! `TraversalExplanation`, and Neptune's `/gremlin/profile` and `/gremlin/explain` text.

use serde::Serialize;

use crate::{GKey, GValue, Metrics};

/// One step's share of a profiled traversal.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepMetric {
    /// The server's step name, e.g. `HasStep([~label.eq(person)])`.
    pub name: String,
    /// Elements the step produced (`elementCount`); `None` when not reported.
    pub count: Option<i64>,
    /// Traversers (bulked elements) the step produced.
    pub traversers: Option<i64>,
    pub dur_ms: f64,
    /// Share of the parent's time, 0–100.
    pub pct: f64,
    /// Steps of child traversals (`repeat`, `where`, `by`...).
    pub children: Vec<StepMetric>,
    /// Other annotations, as text.
    pub annotations: Vec<(String, String)>,
}

/// A titled block of provider output kept as text (Neptune's physical pipeline, index operations...).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Section {
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct ProfileReport {
    pub total_ms: f64,
    pub steps: Vec<StepMetric>,
    /// The traversal after strategies (Neptune reports it with the profile).
    pub optimized: Option<String>,
    pub sections: Vec<Section>,
}

impl ProfileReport {
    /// From TinkerPop `profile()` output.
    pub fn from_metrics(m: &Metrics) -> Self {
        let total = m.dur_ms;
        let steps = step_metrics(&m.metrics, total);
        Self {
            total_ms: if total > 0.0 {
                total
            } else {
                steps.iter().map(|s| s.dur_ms).sum()
            },
            steps,
            optimized: None,
            sections: Vec::new(),
        }
    }

    /// From the items of a `profile()` request.
    pub fn from_items(items: &[GValue]) -> Option<Self> {
        items.iter().find_map(|v| match v {
            GValue::TraversalMetrics(m) | GValue::Metrics(m) => Some(Self::from_metrics(m)),
            _ => None,
        })
    }

    /// From Neptune's `/gremlin/profile` text report.
    pub fn from_neptune_text(text: &str) -> Self {
        let sections = split_sections(text);
        let mut report = Self::default();
        for s in &sections {
            match s.title.as_str() {
                "Traversal Metrics" => {
                    let (steps, total) = parse_metrics_table(&s.text);
                    report.total_ms = total.unwrap_or_else(|| steps.iter().map(|s| s.dur_ms).sum());
                    report.steps = steps;
                }
                "Optimized Traversal" => report.optimized = Some(s.text.clone()),
                _ => {}
            }
        }
        report.sections = sections
            .into_iter()
            .filter(|s| {
                !matches!(
                    s.title.as_str(),
                    "Traversal Metrics" | "Optimized Traversal"
                )
            })
            .collect();
        report
    }

    /// Index path of the step with the highest time share, searching the top-level steps.
    pub fn bottleneck(&self) -> Option<usize> {
        self.steps
            .iter()
            .enumerate()
            .filter(|(_, s)| s.pct > 0.0)
            .max_by(|a, b| a.1.pct.total_cmp(&b.1.pct))
            .map(|(i, _)| i)
    }
}

fn step_metrics(metrics: &[Metrics], parent_total: f64) -> Vec<StepMetric> {
    let total = if parent_total > 0.0 {
        parent_total
    } else {
        metrics.iter().map(|m| m.dur_ms).sum::<f64>()
    };
    metrics
        .iter()
        .map(|m| {
            let pct = m
                .annotations
                .get("percentDur")
                .and_then(number)
                .unwrap_or_else(|| {
                    if total > 0.0 {
                        m.dur_ms / total * 100.0
                    } else {
                        0.0
                    }
                });
            StepMetric {
                name: m.name.clone().unwrap_or_else(|| "?".into()),
                count: m.counts.get("elementCount").copied(),
                traversers: m.counts.get("traverserCount").copied(),
                dur_ms: m.dur_ms,
                pct,
                children: step_metrics(&m.metrics, m.dur_ms),
                annotations: m
                    .annotations
                    .iter()
                    .filter(|(k, _)| k.as_str() != "percentDur")
                    .map(|(k, v)| (k.clone(), crate::console::format(v)))
                    .collect(),
            }
        })
        .collect()
}

fn number(v: &GValue) -> Option<f64> {
    match v {
        GValue::Double(d) => Some(*d),
        GValue::Float(f) => Some(f64::from(*f)),
        other => other.as_i64().map(|i| i as f64),
    }
}

/// Split Neptune's report into sections: a title line underlined with `=`.
pub fn split_sections(text: &str) -> Vec<Section> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<Section> = Vec::new();
    let mut body: Vec<&str> = Vec::new();
    let mut title: Option<String> = None;
    let mut i = 0;
    let flush = |title: &mut Option<String>, body: &mut Vec<&str>, out: &mut Vec<Section>| {
        if let Some(t) = title.take() {
            out.push(Section {
                title: t,
                text: body.join("\n").trim_matches('\n').trim_end().to_owned(),
            });
        }
        body.clear();
    };
    while i < lines.len() {
        let line = lines[i];
        let underline = lines
            .get(i + 1)
            .is_some_and(|n| n.len() >= 3 && n.trim().chars().all(|c| c == '='));
        if underline && !line.trim().is_empty() {
            flush(&mut title, &mut body, &mut out);
            title = Some(line.trim().to_owned());
            i += 2;
            continue;
        }
        if title.is_some() {
            body.push(line);
        }
        i += 1;
    }
    flush(&mut title, &mut body, &mut out);
    out
}

/// Neptune's "Traversal Metrics" table: name, count, traversers, time (ms), % dur; and the `>TOTAL` row.
fn parse_metrics_table(text: &str) -> (Vec<StepMetric>, Option<f64>) {
    let mut steps = Vec::new();
    let mut total = None;
    let mut past_header = false;
    for line in text.lines() {
        if line.trim_start().starts_with("---") {
            past_header = true;
            continue;
        }
        if !past_header || line.trim().is_empty() {
            continue;
        }
        // The name may contain single spaces; columns are separated by runs of spaces.
        let cols: Vec<&str> = line
            .split("  ")
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .collect();
        if cols.len() < 5 {
            continue;
        }
        let n = cols.len();
        let name = cols[..n - 4].join(" ");
        let int = |s: &str| s.replace(',', "").parse::<i64>().ok();
        let float = |s: &str| s.replace(',', "").parse::<f64>().ok();
        if name.trim_start_matches('>') == "TOTAL" {
            total = float(cols[n - 2]);
            continue;
        }
        steps.push(StepMetric {
            name,
            count: int(cols[n - 4]),
            traversers: int(cols[n - 3]),
            dur_ms: float(cols[n - 2]).unwrap_or(0.0),
            pct: float(cols[n - 1]).unwrap_or(0.0),
            children: Vec::new(),
            annotations: Vec::new(),
        });
    }
    (steps, total)
}

/// One strategy application in a TinkerPop explanation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StrategyStep {
    pub strategy: String,
    /// `decoration`, `optimization`, `provider`, `finalization`, `verification`.
    pub category: String,
    /// The traversal after this strategy.
    pub traversal: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct ExplainReport {
    pub original: Option<String>,
    pub strategies: Vec<StrategyStep>,
    /// The traversal that runs.
    pub final_traversal: Option<String>,
    /// Provider output kept as text (Neptune).
    pub sections: Vec<Section>,
}

impl ExplainReport {
    /// From TinkerPop `explain()` output: a map of `original`, `intermediate` and `final`.
    pub fn from_items(items: &[GValue]) -> Option<Self> {
        // GraphSON 3 wraps the map as `g:TraversalExplanation`, which decodes as an unknown type.
        let decoded = items.iter().find_map(|v| match v {
            GValue::Map(_) => Some(v.clone()),
            GValue::Unknown { type_name, value } if type_name.ends_with("TraversalExplanation") => {
                crate::graphson::decode(value).ok()
            }
            _ => None,
        })?;
        let GValue::Map(map) = &decoded else {
            return None;
        };
        let get = |k: &str| map.get(&GKey::String(k.into()));
        let steps = |v: Option<&GValue>| match v {
            Some(GValue::List(l)) => Some(
                l.iter()
                    .map(crate::console::format)
                    .collect::<Vec<_>>()
                    .join(", "),
            )
            .map(|s| format!("[{s}]")),
            Some(other) => Some(crate::console::format(other)),
            None => None,
        };
        let strategies = match get("intermediate") {
            Some(GValue::List(l)) => l
                .iter()
                .filter_map(|e| match e {
                    GValue::Map(m) => {
                        let s = |k: &str| {
                            m.get(&GKey::String(k.into()))
                                .map(crate::console::format)
                                .unwrap_or_default()
                        };
                        Some(StrategyStep {
                            strategy: s("strategy"),
                            category: category(&s("category")),
                            traversal: steps(m.get(&GKey::String("traversal".into())))
                                .unwrap_or_default(),
                        })
                    }
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        if get("original").is_none() && get("final").is_none() {
            return None;
        }
        Some(Self {
            original: steps(get("original")),
            strategies,
            final_traversal: steps(get("final")),
            sections: Vec::new(),
        })
    }

    /// From Neptune's `/gremlin/explain` text report.
    pub fn from_neptune_text(text: &str) -> Self {
        let sections = split_sections(text);
        let find = |t: &str| {
            sections
                .iter()
                .find(|s| s.title == t)
                .map(|s| s.text.clone())
        };
        Self {
            original: find("Original Traversal"),
            strategies: Vec::new(),
            final_traversal: find("Optimized Traversal").or_else(|| find("Converted Traversal")),
            sections: sections
                .iter()
                .filter(|s| {
                    !matches!(
                        s.title.as_str(),
                        "Original Traversal" | "Optimized Traversal"
                    )
                })
                .cloned()
                .collect(),
        }
    }
}

/// `DecorationStrategy` → `decoration`.
fn category(raw: &str) -> String {
    raw.trim_end_matches("Strategy").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn neptune_sections_and_table() {
        let r = ProfileReport::from_neptune_text(include_str!(
            "../../../testenv/fixtures/neptune/profile-1.0.txt"
        ));
        assert_eq!(r.steps.len(), 4);
        assert_eq!(r.steps[2].name, "RangeGlobalStep(0,5)");
        assert_eq!(r.steps[3].name, "PropertiesStep([city],value)");
        assert!((r.total_ms - 1.523).abs() < 1e-9);
        assert!(
            r.optimized
                .unwrap()
                .contains("not converted into Neptune steps")
        );
        assert!(r.sections.iter().any(|s| s.title == "Runtime (ms)"));
    }

    #[test]
    fn metrics_mapping_uses_percent_annotation() {
        let child = |name: &str, dur: f64, pct: f64| Metrics {
            name: Some(name.into()),
            dur_ms: dur,
            counts: [
                ("elementCount".to_owned(), 3),
                ("traverserCount".to_owned(), 2),
            ]
            .into_iter()
            .collect(),
            annotations: [("percentDur".to_owned(), GValue::Double(pct))]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let m = Metrics {
            dur_ms: 10.0,
            metrics: vec![child("A", 7.0, 70.0), child("B", 3.0, 30.0)],
            ..Default::default()
        };
        let r = ProfileReport::from_metrics(&m);
        assert_eq!(r.bottleneck(), Some(0));
        assert_eq!(r.steps[0].count, Some(3));
        assert_eq!(r.steps[0].traversers, Some(2));
    }
}

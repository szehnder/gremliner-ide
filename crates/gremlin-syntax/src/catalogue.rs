//! The step catalogue (`steps.toml`): what each standard Gremlin step is, its signatures and docs.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

const REFERENCE: &str = "https://tinkerpop.apache.org/docs/current/reference/";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepKind {
    Source,
    Filter,
    Map,
    FlatMap,
    SideEffect,
    Branch,
    Barrier,
    Modulator,
    Terminal,
    Mutation,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepInfo {
    pub name: String,
    pub kind: StepKind,
    #[serde(default)]
    pub mutating: bool,
    pub signatures: Vec<String>,
    pub doc: String,
    #[serde(default)]
    anchor: Option<String>,
}

impl StepInfo {
    /// The TinkerPop reference documentation for this step.
    pub fn link(&self) -> String {
        let anchor = self
            .anchor
            .clone()
            .unwrap_or_else(|| format!("{}-step", self.name.to_lowercase()));
        format!("{REFERENCE}#{anchor}")
    }
}

#[derive(Debug, Deserialize)]
struct Tokens {
    predicates: Vec<String>,
    classes: Vec<String>,
    statics: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct File {
    step: Vec<StepInfo>,
    tokens: Tokens,
}

pub struct Catalogue {
    steps: Vec<StepInfo>,
    by_name: HashMap<String, usize>,
    predicates: Vec<String>,
    classes: Vec<String>,
    statics: Vec<String>,
}

static CATALOGUE: LazyLock<Catalogue> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    let file: File = toml::from_str(include_str!("../steps.toml")).expect("steps.toml is valid");
    let by_name = file
        .step
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name.clone(), i))
        .collect();
    Catalogue {
        steps: file.step,
        by_name,
        predicates: file.tokens.predicates,
        classes: file.tokens.classes,
        statics: file.tokens.statics,
    }
});

impl Catalogue {
    pub fn get() -> &'static Catalogue {
        &CATALOGUE
    }

    pub fn steps(&self) -> &[StepInfo] {
        &self.steps
    }

    pub fn step(&self, name: &str) -> Option<&StepInfo> {
        self.by_name.get(name).map(|&i| &self.steps[i])
    }

    pub fn is_step(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    pub fn is_predicate(&self, name: &str) -> bool {
        self.predicates.iter().any(|p| p == name)
    }

    /// `P`, `T`, `Order`, ...: roots whose calls are not traversal steps.
    pub fn is_token_class(&self, name: &str) -> bool {
        self.classes.iter().any(|c| c == name)
    }

    /// Statically imported tokens such as `local`, `desc`, `label`.
    pub fn is_static_token(&self, name: &str) -> bool {
        self.statics.iter().any(|s| s == name)
    }

    /// Any name Gremlin knows: steps, predicates, token classes, static tokens.
    pub fn is_known_name(&self, name: &str) -> bool {
        self.is_step(name)
            || self.is_predicate(name)
            || self.is_token_class(name)
            || self.is_static_token(name)
    }
}

/// Hover documentation for a step.
pub fn documentation(step: &str) -> Option<&'static StepInfo> {
    Catalogue::get().step(step)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_gremlin_language_hover_documentation() {
        let repeat = documentation("repeat").unwrap();
        assert!(repeat.doc.contains("Loops"));
        assert!(
            repeat
                .signatures
                .iter()
                .any(|s| s.starts_with("repeat(Traversal"))
        );
        assert_eq!(
            repeat.link(),
            "https://tinkerpop.apache.org/docs/current/reference/#repeat-step"
        );
        assert_eq!(
            documentation("out").unwrap().link(),
            format!("{REFERENCE}#vertex-steps")
        );
        assert!(documentation("nope").is_none());
    }

    #[test]
    fn catalogue_is_consistent() {
        let c = Catalogue::get();
        assert!(c.steps().len() > 140, "{}", c.steps().len());
        for s in c.steps() {
            assert!(!s.signatures.is_empty() && !s.doc.is_empty(), "{}", s.name);
            assert!(
                s.signatures
                    .iter()
                    .all(|sig| sig.starts_with(&format!("{}(", s.name))),
                "{}",
                s.name
            );
            assert_eq!(
                s.mutating,
                s.kind == StepKind::Mutation || s.mutating,
                "{}",
                s.name
            );
        }
        for m in ["addV", "addE", "property", "drop", "mergeV", "mergeE"] {
            assert!(c.step(m).unwrap().mutating, "{m}");
        }
    }
}

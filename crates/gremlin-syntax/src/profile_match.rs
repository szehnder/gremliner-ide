//! Match profile rows (server step names such as `HasStep([name.eq(josh)])`) to the source steps they
//! came from, so selecting a row can highlight the query. Matching is in order by step kind; strategies
//! fold some steps (a `has` into `TinkerGraphStep`), so a matched row also covers the `has` filters that
//! follow it without a row of their own. Rows that cannot be matched get `None`.

use std::ops::Range;

use crate::document::{Document, StepNode};

/// Source step names a server step class can come from.
fn sources(class: &str) -> &'static [&'static str] {
    match class {
        "GraphStep" | "TinkerGraphStep" | "NeptuneGraphQueryStep" | "JanusGraphStep" => &["V", "E"],
        "VertexStep" => &["out", "in", "both", "outE", "inE", "bothE"],
        "EdgeVertexStep" => &["inV", "outV", "bothV"],
        "EdgeOtherVertexStep" => &["otherV"],
        "HasStep" => &["has", "hasLabel", "hasId", "hasKey", "hasValue", "hasNot"],
        "NotStep" => &["not", "hasNot"],
        "TraversalFilterStep" => &["filter", "where", "has"],
        "PropertiesStep" => &["values", "properties"],
        "PropertyMapStep" => &["valueMap", "propertyMap"],
        "ElementMapStep" => &["elementMap"],
        "PathStep" => &["path"],
        "CountGlobalStep" | "CountLocalStep" | "NeptuneCountGlobalStep" => &["count"],
        "RangeGlobalStep" | "RangeLocalStep" => &["limit", "range", "skip"],
        "TailGlobalStep" | "TailLocalStep" => &["tail"],
        "DedupGlobalStep" | "DedupLocalStep" => &["dedup"],
        "OrderGlobalStep" | "OrderLocalStep" => &["order"],
        "GroupCountStep" | "GroupCountSideEffectStep" => &["groupCount"],
        "GroupStep" | "GroupSideEffectStep" => &["group"],
        "ProjectStep" => &["project"],
        "SelectOneStep" | "SelectStep" | "TraversalSelectStep" => &["select"],
        "WherePredicateStep" | "WhereTraversalStep" => &["where"],
        "RepeatStep" => &["repeat"],
        "UnionStep" => &["union"],
        "CoalesceStep" => &["coalesce"],
        "ChooseStep" | "BranchStep" => &["choose", "branch"],
        "OptionalStep" => &["optional"],
        "AndStep" => &["and"],
        "OrStep" => &["or"],
        "IdStep" => &["id"],
        "LabelStep" => &["label"],
        "FoldStep" => &["fold"],
        "UnfoldStep" => &["unfold"],
        "SumGlobalStep" | "SumLocalStep" => &["sum"],
        "MeanGlobalStep" | "MeanLocalStep" => &["mean"],
        "MaxGlobalStep" | "MaxLocalStep" => &["max"],
        "MinGlobalStep" | "MinLocalStep" => &["min"],
        "AddVertexStep" | "AddVertexStartStep" => &["addV"],
        "AddEdgeStep" | "AddEdgeStartStep" => &["addE"],
        "AddPropertyStep" => &["property"],
        "DropStep" => &["drop"],
        "IsStep" => &["is"],
        "ConstantStep" => &["constant"],
        "InjectStep" => &["inject"],
        "IdentityStep" => &["identity"],
        "PathFilterStep" => &["simplePath", "cyclicPath"],
        "AggregateGlobalStep" | "AggregateLocalStep" => &["aggregate", "store"],
        "SideEffectCapStep" => &["cap"],
        "TreeStep" | "TreeSideEffectStep" => &["tree"],
        "MatchStep" => &["match"],
        "LoopsStep" => &["loops"],
        "SampleGlobalStep" => &["sample"],
        "CoinStep" => &["coin"],
        "MathStep" => &["math"],
        "ConcatStep" => &["concat"],
        "MergeVertexStep" => &["mergeV"],
        "MergeEdgeStep" => &["mergeE"],
        _ => &[],
    }
}

const HAS_FAMILY: [&str; 6] = ["has", "hasLabel", "hasId", "hasKey", "hasValue", "hasNot"];

/// The class part of a server step name: `HasStep([x])` → `HasStep`.
fn class_of(name: &str) -> &str {
    let end = name.find(['(', '@', ' ', '[']).unwrap_or(name.len());
    &name[..end]
}

/// For each profile row (top level, in order), the byte range of the source it came from in `text`
/// (the first statement), or `None`.
pub fn profile_step_ranges(text: &str, rows: &[&str]) -> Vec<Option<Range<usize>>> {
    let doc = Document::parse(text);
    let Some(statement) = doc.statements().into_iter().next() else {
        return vec![None; rows.len()];
    };
    let steps: Vec<StepNode> = doc
        .steps(&statement)
        .into_iter()
        .filter(|s| s.depth == 0 && s.is_gremlin())
        .collect();
    let mut used = vec![false; steps.len()];
    let mut matched: Vec<Option<usize>> = Vec::with_capacity(rows.len());
    let mut next = 0;
    for row in rows {
        let allowed = sources(class_of(row));
        let found =
            (next..steps.len()).find(|&i| !used[i] && allowed.contains(&steps[i].name.as_str()));
        if let Some(i) = found {
            used[i] = true;
            next = i + 1;
        }
        matched.push(found);
    }
    matched
        .into_iter()
        .map(|m| {
            let i = m?;
            let mut end = steps[i].span.end;
            // Absorb following `has` filters folded into this step (no row of their own).
            let mut j = i + 1;
            while j < steps.len() && !used[j] && HAS_FAMILY.contains(&steps[j].name.as_str()) {
                end = steps[j].span.end;
                j += 1;
            }
            Some(steps[i].span.start..end)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts<'a>(text: &'a str, rows: &[&str]) -> Vec<Option<&'a str>> {
        profile_step_ranges(text, rows)
            .into_iter()
            .map(|r| r.map(|r| &text[r]))
            .collect()
    }

    #[test]
    fn rows_map_to_calls() {
        let q = "g.V().out('knows').has('name','josh').values('age')";
        assert_eq!(
            texts(
                q,
                &[
                    "TinkerGraphStep(vertex,[])",
                    "VertexStep(OUT,[knows],vertex)",
                    "HasStep([name.eq(josh)])",
                    "PropertiesStep([age],value)",
                    "ReferenceElementStep",
                ]
            ),
            vec![
                Some("V()"),
                Some("out('knows')"),
                Some("has('name','josh')"),
                Some("values('age')"),
                None
            ]
        );
    }

    #[test]
    fn folded_has_steps_join_the_graph_step() {
        let q = "g.V().hasLabel('person').has('age', gt(30)).count()";
        assert_eq!(
            texts(
                q,
                &[
                    "TinkerGraphStep(vertex,[~label.eq(person), age.gt(30)])",
                    "CountGlobalStep"
                ]
            ),
            vec![
                Some("V().hasLabel('person').has('age', gt(30))"),
                Some("count()")
            ]
        );
    }
}

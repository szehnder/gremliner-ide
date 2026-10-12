//! Loading benchmark datasets: the node/edge structure of a GraphML file (enough for `air-routes.graphml`
//! and the synthetic testenv graphs; attributes are ignored).

use std::collections::HashMap;

use crate::LayoutGraph;

/// Parses `<node id='…'>` and `<edge … source='…' target='…'>` elements.
pub fn parse_graphml(text: &str) -> LayoutGraph {
    let mut ids: HashMap<&str, u32> = HashMap::new();
    let mut edges = Vec::new();
    let mut pending: Vec<(&str, &str)> = Vec::new();
    for tag in text.split('<').skip(1) {
        if let Some(rest) = tag.strip_prefix("node ") {
            if let Some(id) = attr(rest, "id") {
                let next = ids.len() as u32;
                ids.entry(id).or_insert(next);
            }
        } else if let Some(rest) = tag.strip_prefix("edge ")
            && let (Some(s), Some(t)) = (attr(rest, "source"), attr(rest, "target"))
        {
            pending.push((s, t));
        }
    }
    for (s, t) in pending {
        if let (Some(a), Some(b)) = (ids.get(s), ids.get(t)) {
            edges.push([*a, *b]);
        }
    }
    LayoutGraph::new(ids.len(), edges)
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let head = tag.split('>').next()?;
    for quote in ['\'', '"'] {
        let key = format!("{name}={quote}");
        if let Some(start) = head.find(&key) {
            let rest = &head[start + key.len()..];
            return rest.find(quote).map(|end| &rest[..end]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nodes_and_edges_with_either_quote() {
        let g = parse_graphml(
            r#"<graphml><graph><node id='a'><data key='x'>1</data></node><node id="b"/>
            <edge id='e1' source='a' target="b"><data key='w'>2</data></edge>
            <edge id='e2' source='a' target='zz'/></graph></graphml>"#,
        );
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edges(), &[[0, 1]], "edges to unknown nodes are skipped");
    }
}

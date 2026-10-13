//! Neptune engine version → TinkerPop version (`versions.toml`).

use std::collections::BTreeMap;
use std::sync::LazyLock;

#[derive(serde::Deserialize)]
struct Table {
    engines: BTreeMap<String, String>,
}

/// (engine as numbers, TinkerPop version), ascending.
static ENGINES: LazyLock<Vec<(Vec<u32>, String)>> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    let table: Table = toml::from_str(include_str!("versions.toml")).expect("versions.toml parses");
    let mut rows: Vec<_> = table
        .engines
        .into_iter()
        .filter_map(|(engine, tp)| Some((engine_parts(&engine)?, tp)))
        .collect();
    rows.sort();
    rows
});

/// `1.2.1.0.R7` → [1, 2, 1, 0].
fn engine_parts(v: &str) -> Option<Vec<u32>> {
    let parts: Vec<u32> = v.trim().split('.').map_while(|p| p.parse().ok()).collect();
    (!parts.is_empty()).then_some(parts)
}

/// The TinkerPop version of a Neptune engine release; `None` for versions older than the table.
pub fn tinkerpop_for(engine: &str) -> Option<String> {
    let v = engine_parts(engine)?;
    ENGINES
        .iter()
        .rev()
        .find(|(e, _)| *e <= v)
        .map(|(_, tp)| tp.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greatest_entry_at_or_below() {
        assert_eq!(tinkerpop_for("1.2.1.0").as_deref(), Some("3.6.2"));
        assert_eq!(tinkerpop_for("1.2.1.1.R2").as_deref(), Some("3.6.2"));
        assert_eq!(tinkerpop_for("1.2.0.2").as_deref(), Some("3.5.2"));
        assert_eq!(tinkerpop_for("1.4.5.0").as_deref(), Some("3.7.1"));
        assert_eq!(tinkerpop_for("0.9"), None);
        assert_eq!(tinkerpop_for("garbage"), None);
    }
}

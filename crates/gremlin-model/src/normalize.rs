//! One normalisation pass shared by every results view: what shape a result has, its table model, and a
//! lazily expanded tree model.

use std::cmp::Ordering;
use std::collections::HashSet;

use crate::{GKey, GValue, IndexMap, console, new_map};

/// The overall shape of a result list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultShape {
    Empty,
    /// Vertices and edges (possibly mixed with properties).
    Elements,
    /// Only vertex properties and properties.
    Properties,
    Paths,
    /// Maps (valueMap, elementMap, project, group, ...).
    Maps,
    /// Numbers, strings, booleans, ids...
    Scalars,
    /// Lists, trees or a mixture of kinds.
    Mixed,
}

/// Which view to open first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferredView {
    Graph,
    Table,
    Json,
    Console,
}

impl ResultShape {
    pub fn preferred_view(self) -> PreferredView {
        match self {
            ResultShape::Elements | ResultShape::Paths => PreferredView::Graph,
            ResultShape::Properties | ResultShape::Maps => PreferredView::Table,
            ResultShape::Mixed => PreferredView::Json,
            ResultShape::Scalars | ResultShape::Empty => PreferredView::Console,
        }
    }
}

fn is_element(v: &GValue) -> bool {
    matches!(
        v,
        GValue::Vertex(_) | GValue::Edge(_) | GValue::VertexProperty(_) | GValue::Property(_)
    )
}

fn is_scalar(v: &GValue) -> bool {
    !matches!(
        v,
        GValue::List(_)
            | GValue::Set(_)
            | GValue::BulkSet(_)
            | GValue::Map(_)
            | GValue::Path(_)
            | GValue::Tree(_)
            | GValue::Traverser { .. }
            | GValue::TraversalMetrics(_)
            | GValue::Metrics(_)
    ) && !is_element(v)
}

pub fn shape(items: &[GValue]) -> ResultShape {
    let Some(first) = items.first() else {
        return ResultShape::Empty;
    };
    let all = |f: fn(&GValue) -> bool| items.iter().all(f);
    if all(is_element) {
        if items
            .iter()
            .any(|v| matches!(v, GValue::Vertex(_) | GValue::Edge(_)))
        {
            ResultShape::Elements
        } else {
            ResultShape::Properties
        }
    } else if all(|v| matches!(v, GValue::Path(_))) {
        ResultShape::Paths
    } else if all(|v| matches!(v, GValue::Map(_))) {
        ResultShape::Maps
    } else if all(is_scalar) {
        ResultShape::Scalars
    } else {
        let _ = first;
        ResultShape::Mixed
    }
}

/// A table cell: what to show and how to sort.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Empty,
    Number(f64, String),
    Text(String),
}

impl Cell {
    fn from_value(v: &GValue) -> Cell {
        match v {
            GValue::Null => Cell::Empty,
            GValue::Int32(n) => Cell::Number(f64::from(*n), n.to_string()),
            #[allow(clippy::cast_precision_loss)]
            GValue::Int64(n) => Cell::Number(*n as f64, n.to_string()),
            GValue::Short(n) => Cell::Number(f64::from(*n), n.to_string()),
            GValue::Byte(n) => Cell::Number(f64::from(*n), n.to_string()),
            GValue::Float(f) => Cell::Number(f64::from(*f), console::format(v)),
            GValue::Double(f) => Cell::Number(*f, console::format(v)),
            // valueMap-style single-element lists show their element.
            GValue::List(items) if items.len() == 1 => Cell::from_value(&items[0]),
            other => Cell::Text(console::format(other)),
        }
    }

    pub fn display(&self) -> &str {
        match self {
            Cell::Empty => "",
            Cell::Number(_, s) | Cell::Text(s) => s,
        }
    }

    /// Numbers before text, empty cells last; numbers numerically, text case-insensitively.
    pub fn compare(&self, other: &Cell) -> Ordering {
        match (self, other) {
            (Cell::Number(a, _), Cell::Number(b, _)) => a.total_cmp(b),
            (Cell::Text(a), Cell::Text(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
            (Cell::Number(..), _) | (Cell::Text(_), Cell::Empty) => Ordering::Less,
            (_, Cell::Number(..)) | (Cell::Empty, Cell::Text(_)) => Ordering::Greater,
            (Cell::Empty, Cell::Empty) => Ordering::Equal,
        }
    }
}

/// One table: a label group of elements, or the whole result for maps and scalars.
#[derive(Debug, Clone, PartialEq)]
pub struct TableGroup {
    /// The label (elements) or a description such as `values`.
    pub label: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
    /// Index into the result items for each row.
    pub items: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableModel {
    pub groups: Vec<TableGroup>,
}

/// Columns are collected in order of first appearance.
struct Builder {
    label: String,
    columns: Vec<String>,
    rows: Vec<IndexMap<String, Cell>>,
    items: Vec<usize>,
}

impl Builder {
    fn new(label: impl Into<String>, fixed: &[&str]) -> Self {
        Self {
            label: label.into(),
            columns: fixed.iter().map(|c| (*c).to_owned()).collect(),
            rows: Vec::new(),
            items: Vec::new(),
        }
    }

    fn push(&mut self, item: usize, row: IndexMap<String, Cell>) {
        for key in row.keys() {
            if !self.columns.contains(key) {
                self.columns.push(key.clone());
            }
        }
        self.rows.push(row);
        self.items.push(item);
    }

    fn finish(self) -> TableGroup {
        let rows = self
            .rows
            .into_iter()
            .map(|mut row| {
                self.columns
                    .iter()
                    .map(|c| row.swap_remove(c).unwrap_or(Cell::Empty))
                    .collect()
            })
            .collect();
        TableGroup {
            label: self.label,
            columns: self.columns,
            rows,
            items: self.items,
        }
    }
}

fn key_text(key: &GKey) -> String {
    console::format(&crate::graphson::key_to_value(key))
}

/// Build the table model for a result.
pub fn table(items: &[GValue]) -> TableModel {
    match shape(items) {
        ResultShape::Empty => TableModel::default(),
        ResultShape::Elements | ResultShape::Properties => element_tables(items),
        ResultShape::Maps => {
            let mut b = Builder::new("values", &[]);
            for (i, item) in items.iter().enumerate() {
                if let GValue::Map(map) = item {
                    let mut row = new_map();
                    for (k, v) in map {
                        row.insert(key_text(k), Cell::from_value(v));
                    }
                    b.push(i, row);
                }
            }
            TableModel {
                groups: vec![b.finish()],
            }
        }
        _ => {
            let mut b = Builder::new("values", &["value"]);
            for (i, item) in items.iter().enumerate() {
                let mut row = new_map();
                row.insert("value".to_owned(), Cell::from_value(item));
                b.push(i, row);
            }
            TableModel {
                groups: vec![b.finish()],
            }
        }
    }
}

fn element_tables(items: &[GValue]) -> TableModel {
    // Groups keyed by (kind, label) in order of first appearance.
    let mut groups: IndexMap<(u8, String), Builder> = new_map();
    for (i, item) in items.iter().enumerate() {
        let mut row = new_map();
        let key = match item {
            GValue::Vertex(v) => {
                row.insert("id".to_owned(), Cell::from_value(&v.id));
                row.insert("label".to_owned(), Cell::Text(v.label.clone()));
                for (k, props) in &v.properties {
                    let cell = match props.as_slice() {
                        [single] => Cell::from_value(&single.value),
                        many => Cell::Text(console::format(&GValue::List(
                            many.iter().map(|p| (*p.value).clone()).collect(),
                        ))),
                    };
                    row.insert(k.clone(), cell);
                }
                (0, v.label.clone())
            }
            GValue::Edge(e) => {
                row.insert("id".to_owned(), Cell::from_value(&e.id));
                row.insert("label".to_owned(), Cell::Text(e.label.clone()));
                row.insert("outV".to_owned(), Cell::from_value(&e.out_v));
                row.insert("inV".to_owned(), Cell::from_value(&e.in_v));
                for (k, p) in &e.properties {
                    row.insert(k.clone(), Cell::from_value(&p.value));
                }
                (1, e.label.clone())
            }
            GValue::VertexProperty(p) => {
                row.insert("id".to_owned(), Cell::from_value(&p.id));
                row.insert("key".to_owned(), Cell::Text(p.label.clone()));
                row.insert("value".to_owned(), Cell::from_value(&p.value));
                for (k, v) in &p.properties {
                    row.insert(k.clone(), Cell::from_value(v));
                }
                (2, "properties".to_owned())
            }
            GValue::Property(p) => {
                row.insert("key".to_owned(), Cell::Text(p.key.clone()));
                row.insert("value".to_owned(), Cell::from_value(&p.value));
                (2, "properties".to_owned())
            }
            _ => continue,
        };
        let fixed: &[&str] = match key.0 {
            0 => &["id", "label"],
            1 => &["id", "label", "outV", "inV"],
            _ => &[],
        };
        groups
            .entry(key.clone())
            .or_insert_with(|| Builder::new(key.1.clone(), fixed))
            .push(i, row);
    }
    TableModel {
        groups: groups.into_values().map(Builder::finish).collect(),
    }
}

/// Rows of `group` in display order after sorting by `column` (if any) and keeping only rows with a cell
/// containing `filter` (case-insensitive).
pub fn view_rows(group: &TableGroup, sort: Option<(usize, bool)>, filter: &str) -> Vec<usize> {
    let needle = filter.trim().to_lowercase();
    let mut rows: Vec<usize> = (0..group.rows.len())
        .filter(|&r| {
            needle.is_empty()
                || group.rows[r]
                    .iter()
                    .any(|c| c.display().to_lowercase().contains(&needle))
        })
        .collect();
    if let Some((col, descending)) = sort {
        rows.sort_by(|&a, &b| {
            let ord = group.rows[a][col].compare(&group.rows[b][col]);
            if descending { ord.reverse() } else { ord }
        });
    }
    rows
}

/// CSV (RFC 4180) for the given rows of a group, with a header row.
pub fn to_csv(group: &TableGroup, rows: &[usize]) -> String {
    fn field(s: &str) -> String {
        if s.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_owned()
        }
    }
    let mut out = group
        .columns
        .iter()
        .map(|c| field(c))
        .collect::<Vec<_>>()
        .join(",");
    out.push_str("\r\n");
    for &r in rows {
        out.push_str(
            &group.rows[r]
                .iter()
                .map(|c| field(c.display()))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push_str("\r\n");
    }
    out
}

/// Tab-separated text for copying cells.
pub fn to_tsv(group: &TableGroup, rows: &[usize]) -> String {
    rows.iter()
        .map(|&r| {
            group.rows[r]
                .iter()
                .map(Cell::display)
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------------------------------------
// Tree model

/// The children of a value in the JSON tree: (key, child).
pub fn children(value: &GValue) -> Vec<(String, &GValue)> {
    match value {
        GValue::List(items) | GValue::Set(items) => items
            .iter()
            .enumerate()
            .map(|(i, v)| (format!("[{i}]"), v))
            .collect(),
        GValue::BulkSet(items) => items
            .iter()
            .enumerate()
            .map(|(i, (v, n))| (format!("[{i}] ×{n}"), v))
            .collect(),
        GValue::Map(map) => map.iter().map(|(k, v)| (key_text(k), v)).collect(),
        GValue::Vertex(v) => {
            let mut out: Vec<(String, &GValue)> = vec![("id".into(), &*v.id)];
            for (k, props) in &v.properties {
                for (i, p) in props.iter().enumerate() {
                    let key = if props.len() == 1 {
                        k.clone()
                    } else {
                        format!("{k}[{i}]")
                    };
                    out.push((key, &*p.value));
                }
            }
            out
        }
        GValue::Edge(e) => {
            let mut out: Vec<(String, &GValue)> = vec![
                ("id".into(), &*e.id),
                ("outV".into(), &*e.out_v),
                ("inV".into(), &*e.in_v),
            ];
            out.extend(e.properties.iter().map(|(k, p)| (k.clone(), &*p.value)));
            out
        }
        GValue::VertexProperty(p) => {
            let mut out: Vec<(String, &GValue)> =
                vec![("id".into(), &*p.id), ("value".into(), &*p.value)];
            out.extend(p.properties.iter().map(|(k, v)| (k.clone(), v)));
            out
        }
        GValue::Property(p) => vec![("value".into(), &*p.value)],
        GValue::Path(p) => p
            .objects
            .iter()
            .enumerate()
            .map(|(i, o)| {
                let labels = p.labels.get(i).filter(|l| !l.is_empty());
                let key = match labels {
                    Some(l) => format!("[{i}] as {}", l.join(", ")),
                    None => format!("[{i}]"),
                };
                (key, o)
            })
            .collect(),
        GValue::Tree(entries) => entries
            .iter()
            .enumerate()
            .flat_map(|(i, (k, v))| [(format!("[{i}].key"), k), (format!("[{i}].value"), v)])
            .collect(),
        GValue::Traverser { value, .. } => vec![("value".into(), &**value)],
        _ => Vec::new(),
    }
}

/// A one-line summary of a value for its tree row.
pub fn preview(value: &GValue) -> String {
    const MAX: usize = 120;
    let text = match value {
        GValue::Vertex(v) => format!("v[{}] {}", console::format(&v.id), v.label),
        GValue::Edge(e) => console::format(value) + " " + &e.label,
        GValue::List(items) | GValue::Set(items) => format!("{} items", items.len()),
        GValue::Map(map) => format!("{} entries", map.len()),
        GValue::Path(p) => format!("{} steps", p.objects.len()),
        GValue::String(s) => format!("\"{s}\""),
        _ => console::format(value),
    };
    if text.chars().count() > MAX {
        let cut: String = text.chars().take(MAX).collect();
        format!("{cut}…")
    } else {
        text
    }
}

/// A visible row of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    /// Child indices from the root list down to this node.
    pub path: Vec<u32>,
    pub key: String,
    pub expandable: bool,
}

impl TreeRow {
    pub fn depth(&self) -> usize {
        self.path.len() - 1
    }
}

/// The node at `path`.
pub fn node_at<'a>(roots: &'a [GValue], path: &[u32]) -> Option<&'a GValue> {
    let (first, rest) = path.split_first()?;
    let mut node = roots.get(*first as usize)?;
    for &ix in rest {
        node = children(node).get(ix as usize)?.1;
    }
    Some(node)
}

/// The visible rows given the expanded node paths. Only expanded nodes are visited, so collapsed results of
/// any size cost one row per top-level item.
pub fn visible_rows(roots: &[GValue], expanded: &HashSet<Vec<u32>>) -> Vec<TreeRow> {
    let mut out = Vec::with_capacity(roots.len());
    for (i, root) in roots.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let path = vec![i as u32];
        push_rows(root, path, format!("[{i}]"), expanded, &mut out);
    }
    out
}

fn push_rows(
    value: &GValue,
    path: Vec<u32>,
    key: String,
    expanded: &HashSet<Vec<u32>>,
    out: &mut Vec<TreeRow>,
) {
    let expandable = !children(value).is_empty();
    let is_open = expandable && expanded.contains(&path);
    out.push(TreeRow {
        path: path.clone(),
        key,
        expandable,
    });
    if is_open {
        for (i, (k, child)) in children(value).into_iter().enumerate() {
            let mut child_path = path.clone();
            #[allow(clippy::cast_possible_truncation)]
            child_path.push(i as u32);
            push_rows(child, child_path, k, expanded, out);
        }
    }
}

/// Every expandable node path (for "expand all"), up to `limit` nodes.
pub fn all_expandable(roots: &[GValue], limit: usize) -> HashSet<Vec<u32>> {
    let mut out = HashSet::new();
    let mut stack: Vec<(Vec<u32>, &GValue)> = roots
        .iter()
        .enumerate()
        .map(|(i, v)| {
            #[allow(clippy::cast_possible_truncation)]
            (vec![i as u32], v)
        })
        .collect();
    while let Some((path, value)) = stack.pop() {
        if out.len() >= limit {
            break;
        }
        let kids = children(value);
        if kids.is_empty() {
            continue;
        }
        for (i, (_, child)) in kids.into_iter().enumerate() {
            let mut p = path.clone();
            #[allow(clippy::cast_possible_truncation)]
            p.push(i as u32);
            stack.push((p, child));
        }
        out.insert(path);
    }
    out
}

/// A readable access path for a row, e.g. `[0].properties.name`.
pub fn access_path(roots: &[GValue], path: &[u32]) -> String {
    let Some((first, rest)) = path.split_first() else {
        return String::new();
    };
    let mut out = format!("[{first}]");
    let Some(mut node) = roots.get(*first as usize) else {
        return out;
    };
    for &ix in rest {
        let kids = children(node);
        let Some((key, child)) = kids.get(ix as usize) else {
            break;
        };
        if key.starts_with('[') {
            out.push_str(key.split(' ').next().unwrap_or(key));
        } else {
            out.push('.');
            out.push_str(key);
        }
        node = child;
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{Path, Property, Vertex, VertexProperty};

    fn vp(label: &str, value: GValue) -> VertexProperty {
        VertexProperty {
            id: Box::new(GValue::Int64(0)),
            label: label.into(),
            value: Box::new(value),
            properties: new_map(),
        }
    }

    fn person(id: i64, name: &str, age: i32) -> GValue {
        let mut properties = new_map();
        properties.insert("name".into(), vec![vp("name", GValue::String(name.into()))]);
        properties.insert("age".into(), vec![vp("age", GValue::Int32(age))]);
        GValue::Vertex(Vertex {
            id: Box::new(GValue::Int64(id)),
            label: "person".into(),
            properties,
        })
    }

    fn software(id: i64, name: &str) -> GValue {
        let mut properties = new_map();
        properties.insert("name".into(), vec![vp("name", GValue::String(name.into()))]);
        properties.insert(
            "lang".into(),
            vec![vp("lang", GValue::String("java".into()))],
        );
        GValue::Vertex(Vertex {
            id: Box::new(GValue::Int64(id)),
            label: "software".into(),
            properties,
        })
    }

    fn modern() -> Vec<GValue> {
        vec![
            person(1, "marko", 29),
            person(2, "vadas", 27),
            software(3, "lop"),
            person(4, "josh", 32),
            software(5, "ripple"),
            person(6, "peter", 35),
        ]
    }

    #[test]
    fn elements_and_paths_prefer_graph() {
        let items: Vec<GValue> = modern()
            .into_iter()
            .filter(|v| matches!(v, GValue::Vertex(x) if x.label == "person"))
            .collect();
        assert_eq!(shape(&items), ResultShape::Elements);
        assert_eq!(shape(&items).preferred_view(), PreferredView::Graph);
        let model = table(&items);
        assert_eq!(model.groups.len(), 1);
        assert_eq!(model.groups[0].rows.len(), 4);
        assert_eq!(
            shape(&[GValue::Int64(1)]).preferred_view(),
            PreferredView::Console
        );
        let path = GValue::Path(Path {
            labels: vec![vec![]],
            objects: vec![person(1, "marko", 29)],
        });
        assert_eq!(shape(&[path]).preferred_view(), PreferredView::Graph);
    }

    #[test]
    fn scenario_results_table_group_by_label() {
        let model = table(&modern());
        let summary: Vec<(String, usize)> = model
            .groups
            .iter()
            .map(|g| (g.label.clone(), g.rows.len()))
            .collect();
        assert_eq!(summary, vec![("person".into(), 4), ("software".into(), 2)]);
        let software = &model.groups[1];
        assert_eq!(software.columns, ["id", "label", "name", "lang"]);
        assert_eq!(software.rows[0][2].display(), "lop");
        assert_eq!(software.rows[1][3].display(), "java");
    }

    #[test]
    fn scenario_results_table_valuemap_results() {
        let items: Vec<GValue> = [("marko", 29), ("vadas", 27), ("josh", 32), ("peter", 35)]
            .iter()
            .map(|(n, a)| {
                let mut m = new_map();
                m.insert(
                    GKey::String("name".into()),
                    GValue::List(vec![GValue::String((*n).into())]),
                );
                m.insert(
                    GKey::String("age".into()),
                    GValue::List(vec![GValue::Int32(*a)]),
                );
                GValue::Map(m)
            })
            .collect();
        let model = table(&items);
        assert_eq!(model.groups[0].columns, ["name", "age"]);
        assert_eq!(model.groups[0].rows.len(), 4);
        assert_eq!(model.groups[0].rows[0][0].display(), "marko");
        // Scalars get a single value column.
        let scalars = table(&[GValue::Int64(1), GValue::String("x".into())]);
        assert_eq!(scalars.groups[0].columns, ["value"]);
    }

    #[test]
    fn sorting_filtering_and_csv() {
        let model = table(&modern());
        let people = &model.groups[0];
        let age = people.columns.iter().position(|c| c == "age").unwrap();
        let name = people.columns.iter().position(|c| c == "name").unwrap();
        let desc = view_rows(people, Some((age, true)), "");
        let names: Vec<&str> = desc
            .iter()
            .map(|&r| people.rows[r][name].display())
            .collect();
        assert_eq!(names, ["peter", "josh", "marko", "vadas"]);
        let filtered = view_rows(people, None, "MAR");
        assert_eq!(filtered.len(), 1);
        let csv = to_csv(people, &view_rows(people, None, ""));
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0], "id,label,name,age");
        assert_eq!(lines[1], "1,person,marko,29");
        let tricky = TableGroup {
            label: "x".into(),
            columns: vec!["a".into()],
            rows: vec![vec![Cell::Text("say \"hi\", ok".into())]],
            items: vec![0],
        };
        assert_eq!(to_csv(&tricky, &[0]), "a\r\n\"say \"\"hi\"\", ok\"\r\n");
    }

    #[test]
    fn tree_rows_expand_lazily() {
        let path = GValue::Path(Path {
            labels: vec![vec!["a".into()], vec![]],
            objects: vec![person(1, "marko", 29), person(2, "vadas", 27)],
        });
        let roots = vec![path];
        let collapsed = visible_rows(&roots, &HashSet::new());
        assert_eq!(collapsed.len(), 1);
        assert!(collapsed[0].expandable);
        let mut expanded = HashSet::new();
        expanded.insert(vec![0]);
        let rows = visible_rows(&roots, &expanded);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].key, "[0] as a");
        assert!(matches!(
            node_at(&roots, &rows[1].path),
            Some(GValue::Vertex(_))
        ));
        expanded.insert(vec![0, 1]);
        let rows = visible_rows(&roots, &expanded);
        assert_eq!(
            rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(),
            ["[0]", "[0] as a", "[1]", "id", "name", "age"]
        );
        assert_eq!(access_path(&roots, &[0, 1, 1]), "[0][1].name");
        assert_eq!(preview(&roots[0]), "2 steps");
        let _ = Property {
            key: String::new(),
            value: Box::new(GValue::Null),
        };
    }
}

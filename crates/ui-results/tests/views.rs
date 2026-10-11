//! Headless tests of the results views on synthetic data. Scenario names map to the results-table,
//! results-json and results-console specs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, WindowOptions, div, px, size,
};
use gremlin_model::{GKey, GValue, Path, Vertex, VertexProperty, new_map, plain};
use ui_results::console_view::ConsoleView;
use ui_results::json_view::JsonView;
use ui_results::table_view::TableView;

struct Host(gpui_kit::AnyView);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        div().size_full().child(self.0.clone())
    }
}

/// Open `build`'s view in a window (wrapped in gpui-kit's Root).
fn mount<V: Render>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut gpui_kit::App) -> Entity<V> + 'static,
) -> (Entity<V>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let mut view = None;
    let (handle, _) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let v = build(window, cx);
                view = Some(v.clone());
                cx.new(|_| Host(v.into()))
            })
        })
        .unwrap();
    let vcx = VisualTestContext::from_window(handle, cx);
    vcx.simulate_resize(size(px(1200.0), px(800.0)));
    vcx.run_until_parked();
    (view.unwrap(), vcx)
}

fn vp(label: &str, value: GValue) -> VertexProperty {
    VertexProperty {
        id: Box::new(GValue::Int64(0)),
        label: label.into(),
        value: Box::new(value),
        properties: new_map(),
    }
}

fn vertex(id: i64, label: &str, props: &[(&str, GValue)]) -> GValue {
    let mut properties = new_map();
    for (k, v) in props {
        properties.insert((*k).to_owned(), vec![vp(k, v.clone())]);
    }
    GValue::Vertex(Vertex {
        id: Box::new(GValue::Int64(id)),
        label: label.into(),
        properties,
    })
}

fn modern() -> Vec<GValue> {
    let s = |v: &str| GValue::String(v.into());
    vec![
        vertex(
            1,
            "person",
            &[("name", s("marko")), ("age", GValue::Int32(29))],
        ),
        vertex(
            2,
            "person",
            &[("name", s("vadas")), ("age", GValue::Int32(27))],
        ),
        vertex(3, "software", &[("name", s("lop")), ("lang", s("java"))]),
        vertex(
            4,
            "person",
            &[("name", s("josh")), ("age", GValue::Int32(32))],
        ),
        vertex(5, "software", &[("name", s("ripple")), ("lang", s("java"))]),
        vertex(
            6,
            "person",
            &[("name", s("peter")), ("age", GValue::Int32(35))],
        ),
    ]
}

fn table_rows(view: &Entity<TableView>, col: &str, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| {
        let table = view.read(cx).table().unwrap().read(cx);
        let d = table.delegate();
        let c = d.group().columns.iter().position(|x| x == col).unwrap();
        (0..d.order().len())
            .map(|r| d.cell(r, c).to_owned())
            .collect()
    })
}

fn click(id: &str, cx: &mut VisualTestContext) {
    let id = gpui_kit::SharedString::from(id.to_owned());
    cx.update(move |window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn scenario_results_table_group_by_label(cx: &mut TestAppContext) {
    let items = modern();
    let (view, mut cx) = mount(cx, move |window, cx| {
        cx.new(|cx| TableView::new(&items, window, cx))
    });
    let labels: Vec<String> = cx.update(|_, cx| {
        view.read(cx)
            .model()
            .groups
            .iter()
            .map(|g| format!("{} ({})", g.label, g.rows.len()))
            .collect()
    });
    assert_eq!(labels, ["person (4)", "software (2)"]);
    click("label-software", &mut cx);
    assert_eq!(table_rows(&view, "name", &mut cx), ["lop", "ripple"]);
    assert_eq!(table_rows(&view, "lang", &mut cx), ["java", "java"]);
}

#[gpui_kit::test]
fn scenario_results_table_sort_by_column(cx: &mut TestAppContext) {
    let items = modern();
    let (view, mut cx) = mount(cx, move |window, cx| {
        cx.new(|cx| TableView::new(&items, window, cx))
    });
    click("th-age", &mut cx);
    assert_eq!(
        table_rows(&view, "age", &mut cx),
        ["27", "29", "32", "35"],
        "first click: ascending"
    );
    click("th-age", &mut cx);
    assert_eq!(
        table_rows(&view, "age", &mut cx),
        ["35", "32", "29", "27"],
        "second click: descending"
    );
}

#[gpui_kit::test]
fn scenario_results_table_quick_filter(cx: &mut TestAppContext) {
    let items = modern();
    let (view, mut cx) = mount(cx, move |window, cx| {
        cx.new(|cx| TableView::new(&items, window, cx))
    });
    cx.update(|window, cx| {
        let input = view.read(cx).filter_input().clone();
        input.update(cx, |s, cx| s.focus(window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.input("mar", cx));
    cx.run_until_parked();
    assert_eq!(table_rows(&view, "name", &mut cx), ["marko"]);
}

#[gpui_kit::test]
fn scenario_results_table_export_csv(cx: &mut TestAppContext) {
    let items = modern();
    let (view, mut cx) = mount(cx, move |window, cx| {
        cx.new(|cx| TableView::new(&items, window, cx))
    });
    let csv = cx.update(|_, cx| view.read(cx).csv(cx)).unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 5, "header + four person rows");
    assert_eq!(lines[0], "id,label,name,age");
    assert_eq!(lines[4], "6,person,peter,35");
    let json = cx.update(|_, cx| view.read(cx).json(cx)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 4);
    assert_eq!(parsed[0]["name"], "marko");
}

#[gpui_kit::test]
fn scenario_results_table_scroll_100k_rows(cx: &mut TestAppContext) {
    let items: Vec<GValue> = (0..100_000)
        .map(|i| {
            let mut m = new_map();
            m.insert(GKey::String("id".into()), GValue::Int64(i));
            m.insert(
                GKey::String("name".into()),
                GValue::String(format!("airport-{i}")),
            );
            m.insert(
                GKey::String("runways".into()),
                GValue::Int32((i % 7) as i32),
            );
            GValue::Map(m)
        })
        .collect();
    let (view, mut cx) = mount(cx, move |window, cx| {
        cx.new(|cx| TableView::new(&items, window, cx))
    });
    let mut worst = Duration::ZERO;
    for step in 0..60 {
        let row = step * 1_650;
        let started = Instant::now();
        cx.update(|window, cx| {
            let table = view.read(cx).table().unwrap().clone();
            table.update(cx, |t, cx| t.scroll_to_row(row, cx));
            window.render_frame(cx);
        });
        worst = worst.max(started.elapsed());
    }
    eprintln!("100k-row table: worst frame {worst:?}");
    let budget = if cfg!(debug_assertions) { 32 } else { 16 };
    assert!(worst.as_millis() < budget, "worst frame {worst:?}");
}

fn person_path() -> GValue {
    let s = |v: &str| GValue::String(v.into());
    GValue::Path(Path {
        labels: vec![vec![], vec![]],
        objects: vec![
            vertex(1, "person", &[("name", s("marko"))]),
            vertex(3, "software", &[("name", s("lop"))]),
        ],
    })
}

#[gpui_kit::test]
fn scenario_results_json_expand_a_path(cx: &mut TestAppContext) {
    let (view, mut cx) = mount(cx, |_, cx| {
        cx.new(|cx| JsonView::new(vec![person_path()], cx))
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(("json-row", 0usize), cx);
    });
    cx.run_until_parked();
    let rows = cx.update(|_, cx| view.read(cx).rows().to_vec());
    assert_eq!(rows.len(), 3, "the path opens to its two vertices");
    let previews: Vec<String> = cx.update(|_, _| {
        rows[1..]
            .iter()
            .map(|r| {
                gremlin_model::normalize::preview(
                    gremlin_model::normalize::node_at(&[person_path()], &r.path).unwrap(),
                )
            })
            .collect()
    });
    assert_eq!(
        previews,
        ["v[1] person", "v[3] software"],
        "each vertex shows id and label"
    );
}

#[gpui_kit::test]
fn scenario_results_json_huge_result_opens_quickly(cx: &mut TestAppContext) {
    // One million nodes: 1,000 lists of 1,000 numbers.
    let items: Vec<GValue> = (0..1_000)
        .map(|i| GValue::List((0..1_000).map(|j| GValue::Int64(i * 1_000 + j)).collect()))
        .collect();
    let started = Instant::now();
    let (view, mut cx) = mount(cx, move |_, cx| cx.new(|cx| JsonView::new(items, cx)));
    cx.update(|window, cx| window.render_frame(cx));
    let elapsed = started.elapsed();
    eprintln!("1M-node JSON view interactive after {elapsed:?}");
    assert!(elapsed < Duration::from_millis(500), "{elapsed:?}");
    assert_eq!(cx.update(|_, cx| view.read(cx).rows().len()), 1_000);
}

#[gpui_kit::test]
fn scenario_results_json_copy_value(cx: &mut TestAppContext) {
    let v = vertex(1, "person", &[("name", GValue::String("marko".into()))]);
    let expected = serde_json::to_string_pretty(&plain::to_json(&v)).unwrap();
    let (view, mut cx) = mount(cx, move |_, cx| cx.new(|cx| JsonView::new(vec![v], cx)));
    cx.update(|_, cx| view.update(cx, |v, cx| v.select(0, cx)));
    click("json-copy-value", &mut cx);
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert_eq!(clipboard.as_deref(), Some(expected.as_str()));
}

#[gpui_kit::test]
fn scenario_results_console_raw_graphson(cx: &mut TestAppContext) {
    let raw = r#"{"requestId":"x","status":{"code":200},"result":{"data":{"@type":"g:List","@value":[{"@type":"g:Int64","@value":1}]}}}"#.to_owned();
    let items = vec![GValue::Int64(1)];
    let (view, mut cx) = mount(cx, move |window, cx| {
        cx.new(|cx| ConsoleView::new(&items, &[raw], window, cx))
    });
    assert_eq!(cx.update(|_, cx| view.read(cx).lines().to_vec()), ["==>1"]);
    click("console-raw", &mut cx);
    let lines = cx.update(|_, cx| view.read(cx).lines().to_vec());
    assert_eq!(lines[0], "{");
    assert!(
        lines.iter().any(|l| l.contains("\"@type\": \"g:Int64\"")),
        "{lines:?}"
    );
}

#[gpui_kit::test]
fn scenario_results_console_regex_search(cx: &mut TestAppContext) {
    let items = modern();
    let (view, mut cx) = mount(cx, move |window, cx| {
        cx.new(|cx| ConsoleView::new(&items, &[], window, cx))
    });
    click("console-regex", &mut cx);
    cx.update(|window, cx| {
        let input = view.read(cx).search_input().clone();
        input.update(cx, |s, cx| s.focus(window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.input(r"v\[\d+\]", cx));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| view.read(cx).matches().len()),
        6,
        "every vertex token"
    );
    cx.update(|_, cx| view.update(cx, |v, cx| v.step(1, cx)));
    assert_eq!(cx.update(|_, cx| view.read(cx).current_match()), Some(1));
}

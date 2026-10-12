//! Headless tests of the Graph view on synthetic results: selection, hiding, legend toggles, the filter
//! panel, find and the status counts. Scenario names map to the graph-view spec.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, KeyBinding, Modifiers, MouseButton,
    ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, Window,
    WindowOptions, div, px, size,
};
use graph_store::{ExtId, NodeIx, flags};
use gremlin_model::{Edge, GValue, Vertex, VertexProperty, new_map};
use ui_results::graph_view::{self, GraphView, GraphViewEvent};

struct Host(gpui_kit::AnyView);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        div().size_full().child(self.0.clone())
    }
}

fn mount(cx: &mut TestAppContext) -> (Entity<GraphView>, VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.bind_keys([
            KeyBinding::new(
                "delete",
                graph_view::HideSelection,
                Some(graph_view::CANVAS_CONTEXT),
            ),
            KeyBinding::new(
                "secondary-a",
                graph_view::SelectAll,
                Some(graph_view::CANVAS_CONTEXT),
            ),
        ]);
    });
    let mut view = None;
    let (handle, _) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let v = cx.new(|cx| GraphView::new(window, cx));
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

fn vertex(id: i64, label: &str, props: &[(&str, GValue)]) -> GValue {
    let mut properties = new_map();
    for (k, v) in props {
        properties.insert(
            (*k).to_owned(),
            vec![VertexProperty {
                id: Box::new(GValue::Int64(0)),
                label: (*k).into(),
                value: Box::new(v.clone()),
                properties: new_map(),
            }],
        );
    }
    GValue::Vertex(Vertex {
        id: Box::new(GValue::Int64(id)),
        label: label.into(),
        properties,
    })
}

fn edge(id: i64, label: &str, out: (i64, &str), inv: (i64, &str)) -> GValue {
    GValue::Edge(Edge {
        id: Box::new(GValue::Int64(id)),
        label: label.into(),
        out_v: Box::new(GValue::Int64(out.0)),
        out_v_label: out.1.into(),
        in_v: Box::new(GValue::Int64(inv.0)),
        in_v_label: inv.1.into(),
        properties: new_map(),
    })
}

/// The modern graph: six vertices and six edges.
fn modern() -> Vec<GValue> {
    let s = |v: &str| GValue::String(v.into());
    let p = "person";
    let sw = "software";
    vec![
        vertex(1, p, &[("name", s("marko")), ("age", GValue::Int32(29))]),
        vertex(2, p, &[("name", s("vadas")), ("age", GValue::Int32(27))]),
        vertex(3, sw, &[("name", s("lop")), ("lang", s("java"))]),
        vertex(4, p, &[("name", s("josh")), ("age", GValue::Int32(32))]),
        vertex(5, sw, &[("name", s("ripple")), ("lang", s("java"))]),
        vertex(6, p, &[("name", s("peter")), ("age", GValue::Int32(35))]),
        edge(7, "knows", (1, p), (2, p)),
        edge(8, "knows", (1, p), (4, p)),
        edge(9, "created", (1, p), (3, sw)),
        edge(10, "created", (4, p), (5, sw)),
        edge(11, "created", (4, p), (3, sw)),
        edge(12, "created", (6, p), (3, sw)),
    ]
}

/// Ingest as a finished run and stop the layout, so node positions stay put for clicks.
fn ingest(view: &Entity<GraphView>, items: Vec<GValue>, cx: &mut VisualTestContext) {
    ingest_running(view, items, cx);
    cx.update(|_, cx| view.update(cx, |v, cx| v.stop_layout(cx)));
    cx.update(|window, cx| window.render_frame(cx));
}

/// Ingest as a finished run, leaving the layout running.
fn ingest_running(view: &Entity<GraphView>, items: Vec<GValue>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| view.update(cx, |v, cx| v.ingest(1, &items, true, cx)));
    // Lay out and fit.
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

fn node(view: &Entity<GraphView>, id: i64, cx: &mut VisualTestContext) -> NodeIx {
    cx.update(|_, cx| {
        view.read(cx)
            .store(cx)
            .node_by_ext(&ExtId::Int(id))
            .unwrap()
    })
}

fn click_node(
    view: &Entity<GraphView>,
    n: NodeIx,
    modifiers: Modifiers,
    cx: &mut VisualTestContext,
) {
    let at = cx.update(|_, cx| view.read(cx).canvas().read(cx).node_window_position(n));
    cx.simulate_mouse_down(at, MouseButton::Left, modifiers);
    cx.simulate_mouse_up(at, MouseButton::Left, modifiers);
}

fn click(id: &str, cx: &mut VisualTestContext) {
    let id = gpui_kit::SharedString::from(id.to_owned());
    cx.update(move |window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    });
    cx.run_until_parked();
}

fn hidden(view: &Entity<GraphView>, n: NodeIx, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| view.read(cx).store(cx).has_flag(n, flags::HIDDEN))
}

#[gpui_kit::test]
fn scenario_graph_view_hide_selection(cx: &mut TestAppContext) {
    let (view, mut cx) = mount(cx);
    ingest(&view, modern(), &mut cx);
    let (vadas, josh) = (node(&view, 2, &mut cx), node(&view, 4, &mut cx));
    click_node(&view, vadas, Modifiers::none(), &mut cx);
    click_node(&view, josh, Modifiers::shift(), &mut cx);
    assert_eq!(cx.update(|_, cx| view.read(cx).stats(cx).selected), 2);

    cx.simulate_keystrokes("delete");

    assert!(hidden(&view, vadas, &mut cx) && hidden(&view, josh, &mut cx));
    let stats = cx.update(|_, cx| view.read(cx).stats(cx));
    assert_eq!(
        (stats.visible_nodes, stats.nodes),
        (4, 6),
        "hidden, not removed"
    );
    // knows(1→2), knows(1→4), created(4→5), created(4→3) touch the hidden nodes.
    assert_eq!((stats.visible_edges, stats.edges), (2, 6));
    assert_eq!(stats.selected, 0);

    cx.update(|_, cx| view.update(cx, |v, cx| v.unhide_all(cx)));
    assert!(!hidden(&view, vadas, &mut cx));
}

#[gpui_kit::test]
fn scenario_graph_view_toggle_label_visibility(cx: &mut TestAppContext) {
    let (view, mut cx) = mount(cx);
    ingest(&view, modern(), &mut cx);
    click("legend-v-software", &mut cx);
    let (lop, ripple, marko) = (
        node(&view, 3, &mut cx),
        node(&view, 5, &mut cx),
        node(&view, 1, &mut cx),
    );
    assert!(hidden(&view, lop, &mut cx) && hidden(&view, ripple, &mut cx));
    assert!(!hidden(&view, marko, &mut cx));
    let (stats, software_hidden) = cx.update(|_, cx| {
        let v = view.read(cx);
        let legend = v.store(cx).vertex_label_counts();
        let software = legend.iter().find(|l| &*l.name == "software").unwrap();
        (v.stats(cx), software.hidden)
    });
    assert!(software_hidden, "the legend shows the label as hidden");
    assert_eq!(
        stats.visible_edges, 2,
        "created edges disappear with their software ends"
    );

    click("legend-v-software", &mut cx);
    assert!(!hidden(&view, lop, &mut cx));
}

#[gpui_kit::test]
fn filter_panel_shows_only_matching_vertices(cx: &mut TestAppContext) {
    let (view, mut cx) = mount(cx);
    ingest(&view, modern(), &mut cx);
    click("graph-filter-toggle", &mut cx);
    click("filter-label-person", &mut cx);
    let (key, value) = cx.update(|_, cx| {
        let (k, v) = view.read(cx).filter_inputs();
        (k.clone(), v.clone())
    });
    cx.update(|window, cx| {
        key.update(cx, |s, cx| s.focus(window, cx));
    });
    cx.simulate_input("age");
    click("filter-op-Gt", &mut cx);
    cx.update(|window, cx| {
        value.update(cx, |s, cx| s.focus(window, cx));
    });
    cx.simulate_input("30");
    cx.run_until_parked();
    let visible: Vec<i64> = cx.update(|_, cx| {
        let g = view.read(cx).store(cx);
        (0..g.node_count())
            .map(|i| NodeIx(i as u32))
            .filter(|n| !g.has_flag(*n, flags::HIDDEN))
            .map(|n| match g.node_ext(n) {
                ExtId::Int(i) => *i,
                ExtId::Str(_) => -1,
            })
            .collect()
    });
    assert_eq!(visible, vec![4, 6], "josh (32) and peter (35)");
    click("filter-clear", &mut cx);
    assert_eq!(cx.update(|_, cx| view.read(cx).stats(cx).visible_nodes), 6);
}

#[gpui_kit::test]
fn scenario_graph_view_find_a_node(cx: &mut TestAppContext) {
    let (view, mut cx) = mount(cx);
    let s = |v: &str| GValue::String(v.into());
    let airports: Vec<GValue> = ["AUS", "DFW", "LHR", "CDG", "JFK", "SFO"]
        .iter()
        .enumerate()
        .map(|(i, code)| vertex(i as i64 + 1, "airport", &[("code", s(code))]))
        .chain([
            edge(100, "route", (1, "airport"), (2, "airport")),
            edge(101, "route", (2, "airport"), (3, "airport")),
            edge(102, "route", (3, "airport"), (4, "airport")),
        ])
        .collect();
    cx.executor().allow_parking();
    ingest_running(&view, airports, &mut cx);
    settle_layout(&view, &mut cx);
    let lhr = node(&view, 3, &mut cx);
    let search = cx.update(|_, cx| view.read(cx).search_input().clone());
    cx.update(|window, cx| search.update(cx, |s, cx| s.focus(window, cx)));
    cx.simulate_input("lhr");
    cx.simulate_keystrokes("enter");
    // The camera tweens in real time; let it finish.
    std::thread::sleep(Duration::from_millis(400));
    cx.update(|window, cx| window.render_frame(cx));
    cx.update(|_, cx| {
        let v = view.read(cx);
        let canvas = v.canvas().read(cx);
        let target = v.store(cx).position(lhr);
        let cam = canvas.camera();
        assert!(
            (cam.center[0] - target[0]).abs() < 0.01 && (cam.center[1] - target[1]).abs() < 0.01,
            "LHR is centred: camera {:?}, LHR {target:?}",
            cam.center
        );
        assert!(
            v.store(cx).has_flag(lhr, flags::SELECTED),
            "and highlighted"
        );
    });
}

#[gpui_kit::test]
fn scenario_graph_view_counts_update(cx: &mut TestAppContext) {
    let (view, mut cx) = mount(cx);
    let items: Vec<GValue> = (1..=20)
        .map(|i| vertex(i, "thing", &[]))
        .chain((2..=20).map(|i| edge(100 + i, "link", (1, "thing"), (i, "thing"))))
        .collect();
    ingest(&view, items, &mut cx);
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.canvas().update(cx, |c, _| {
                let g = c.store_mut();
                for i in [5, 9, 13] {
                    let n = g.node_by_ext(&ExtId::Int(i)).unwrap();
                    g.set_flag(n, flags::SELECTED, true);
                }
            });
            v.hide_selection(cx);
        })
    });
    let text = cx.update(|_, cx| view.read(cx).stats(cx).text());
    assert!(text.starts_with("17 of 20 nodes"), "{text}");
    assert!(text.contains("16 of 19 edges"), "{text}");
    let shown = cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find("graph-stats").is_some()
    });
    assert!(shown, "the counts are on screen");
}

#[gpui_kit::test]
fn double_click_asks_for_neighbours(cx: &mut TestAppContext) {
    let (view, mut cx) = mount(cx);
    ingest(&view, modern(), &mut cx);
    let requests = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = requests.clone();
    let _sub = cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &GraphViewEvent, _| {
            if let GraphViewEvent::Fetch(r) = event {
                sink.borrow_mut().push(r.script.clone());
            }
        })
    });
    let marko = node(&view, 1, &mut cx);
    let at = cx.update(|_, cx| view.read(cx).canvas().read(cx).node_window_position(marko));
    cx.simulate_event(gpui_kit::MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(gpui_kit::MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: 2,
    });
    assert_eq!(
        requests.borrow().as_slice(),
        ["g.V(1L).bothE().range(0, 101).as('e').otherV().as('v').select('e', 'v')"]
    );
    assert_eq!(cx.update(|_, cx| view.read(cx).pending_fetches()), 1);
}

#[gpui_kit::test]
fn clear_and_accumulate_modes(cx: &mut TestAppContext) {
    let (view, mut cx) = mount(cx);
    let first = vec![vertex(1, "a", &[]), vertex(2, "a", &[])];
    let second = vec![vertex(3, "b", &[])];
    cx.update(|_, cx| view.update(cx, |v, cx| v.ingest(1, &first, true, cx)));
    cx.update(|_, cx| view.update(cx, |v, cx| v.ingest(2, &second, true, cx)));
    assert_eq!(
        cx.update(|_, cx| view.read(cx).stats(cx).nodes),
        1,
        "clear mode replaces"
    );
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.set_accumulate(true, cx);
            v.ingest(3, &first, true, cx);
        })
    });
    assert_eq!(cx.update(|_, cx| view.read(cx).stats(cx).nodes), 3);
}

/// Render frames (real time) until the graph's layout has finished.
fn settle_layout(view: &Entity<GraphView>, cx: &mut VisualTestContext) {
    let started = std::time::Instant::now();
    loop {
        cx.update(|window, cx| window.render_frame(cx));
        if !cx.update(|_, cx| view.read(cx).is_layout_running(cx)) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "layout did not finish"
        );
        std::thread::sleep(Duration::from_millis(8));
    }
}

#[gpui_kit::test]
fn scenario_graph_layouts_expand_keeps_context(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (view, mut cx) = mount(cx);
    let items: Vec<GValue> = (1..=12)
        .map(|i| vertex(i, "airport", &[]))
        .chain((2..=12).map(|i| edge(100 + i, "route", (1, "airport"), (i, "airport"))))
        .collect();
    ingest_running(&view, items, &mut cx);
    assert!(
        cx.update(|_, cx| view.read(cx).is_layout_running(cx)),
        "a new graph is laid out"
    );
    settle_layout(&view, &mut cx);
    let before = cx.update(|_, cx| view.read(cx).store(cx).positions().to_vec());

    // Expand node 5: capture the request and answer it with five new neighbours.
    let request = std::rc::Rc::new(std::cell::RefCell::new(None));
    let sink = request.clone();
    let _sub = cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &GraphViewEvent, _| {
            if let GraphViewEvent::Fetch(r) = event {
                sink.borrow_mut().replace(r.clone());
            }
        })
    });
    let n5 = node(&view, 5, &mut cx);
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.expand(n5, graph_view::ExpandDirection::Both, None, cx)
        })
    });
    let request = request.borrow_mut().take().expect("expand request");
    let rows: Vec<GValue> = (20..25)
        .map(|i| {
            let mut m = new_map();
            m.insert(
                gremlin_model::GKey::String("e".into()),
                edge(200 + i, "route", (5, "airport"), (i, "airport")),
            );
            m.insert(
                gremlin_model::GKey::String("v".into()),
                vertex(i, "airport", &[("code", GValue::String(format!("N{i}")))]),
            );
            GValue::Map(m)
        })
        .collect();
    cx.update(|_, cx| view.update(cx, |v, cx| v.apply_fetch(&request, Ok(rows), cx)));
    settle_layout(&view, &mut cx);

    let (after, zoom, p5) = cx.update(|_, cx| {
        let v = view.read(cx);
        (
            v.store(cx).positions().to_vec(),
            v.canvas().read(cx).camera().zoom,
            v.store(cx).position(n5),
        )
    });
    // Previously placed nodes move by less than 10% of the viewport width (1200 px).
    let limit = 0.1 * 1200.0 / zoom;
    for (i, (a, b)) in before.iter().zip(&after).enumerate() {
        let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        assert!(d < limit, "node {i} moved {d} (limit {limit})");
    }
    // New nodes sit around the expanded node rather than across the graph.
    let hub = after[0];
    for p in &after[before.len()..] {
        let to5 = ((p[0] - p5[0]).powi(2) + (p[1] - p5[1]).powi(2)).sqrt();
        let to_hub = ((p[0] - hub[0]).powi(2) + (p[1] - hub[1]).powi(2)).sqrt();
        assert!(
            to5 < to_hub,
            "a new neighbour is closer to the expanded node than to the hub"
        );
    }
}

#[gpui_kit::test]
fn scenario_graph_stylesheets_switch_stylesheet(cx: &mut TestAppContext) {
    use ui_results::style_library::PRESENTATION_ID;
    let (view, mut cx) = mount(cx);
    ingest(&view, modern(), &mut cx);
    let marko = node(&view, 1, &mut cx);
    let radius =
        |cx: &mut VisualTestContext| cx.update(|_, cx| view.read(cx).store(cx).node_radius(marko));
    assert_eq!(radius(&mut cx), 6.0, "Default: 12 px nodes");
    cx.update(|_, cx| view.update(cx, |v, cx| v.set_stylesheet(PRESENTATION_ID, cx)));
    // Immediately, without another merge or frame.
    assert_eq!(radius(&mut cx), 10.0, "Presentation: 20 px nodes");
    let edge_captions = cx.update(|_, cx| {
        view.read(cx)
            .canvas()
            .read(cx)
            .looks()
            .globals
            .show_edge_captions
    });
    assert!(edge_captions);
}

#[gpui_kit::test]
fn editing_a_label_style_in_the_panel_applies_live(cx: &mut TestAppContext) {
    use graph_store::style::Shape;
    let (view, mut cx) = mount(cx);
    ingest(&view, modern(), &mut cx);
    cx.update(|window, cx| view.update(cx, |v, cx| v.set_show_style_editor(true, window, cx)));
    click("style-v-software", &mut cx);
    click("style-shape-Diamond", &mut cx);
    let shape = cx.update(|_, cx| {
        let v = view.read(cx);
        let l = v.store(cx).vertex_label_ix("software").unwrap();
        v.canvas().read(cx).looks().vertex(l).unwrap().shape
    });
    assert_eq!(shape, Shape::Diamond);
    // A caption template typed into the panel.
    let editor = cx.update(|_, cx| view.read(cx).style_editor().unwrap().clone());
    let lop = node(&view, 3, &mut cx);
    let input = cx.update(|_, cx| editor.read(cx).caption_input().clone());
    cx.update(|window, cx| input.update(cx, |s, cx| s.focus(window, cx)));
    cx.simulate_input("{name} ({lang})");
    cx.run_until_parked();
    let text = cx.update(|_, cx| view.read(cx).store(cx).caption(lop).to_string());
    assert_eq!(text, "lop (java)");
    // Other labels are untouched.
    let marko = node(&view, 1, &mut cx);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).store(cx).caption(marko).to_string()),
        "marko"
    );
}

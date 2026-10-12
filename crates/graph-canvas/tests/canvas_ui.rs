//! Headless UI tests for the graph canvas: real GPUI windows and input dispatch.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gpui_kit::{Modifiers, MouseButton, TestAppContext, point, px};
use graph_canvas::GraphCanvas;
use graph_store::{GraphStore, NodeIx, flags};

fn two_node_store() -> GraphStore {
    let mut g = GraphStore::new();
    let a = g.add_node(1.into(), "person", "marko", [0.0, 0.0]);
    let b = g.add_node(2.into(), "person", "vadas", [200.0, 0.0]);
    g.add_edge(7.into(), a, b, "knows");
    g
}

#[gpui_kit::test]
fn scenario_graph_rendering_drag_pins_a_node(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| GraphCanvas::new(two_node_store(), cx));
    cx.run_until_parked();

    let node = NodeIx(0);
    let (start, zoom, before) = view.read_with(cx, |v, _| {
        (
            v.node_window_position(node),
            v.camera().zoom,
            v.store().position(node),
        )
    });
    let end = point(start.x + px(60.0), start.y + px(30.0));

    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());

    view.read_with(cx, |v, _| {
        let after = v.store().position(node);
        assert!(
            v.store().has_flag(node, flags::PINNED),
            "dragged node is pinned"
        );
        assert!(
            v.store().has_flag(node, flags::SELECTED),
            "dragged node is selected"
        );
        assert!(
            (after[0] - before[0] - 60.0 / zoom).abs() < 0.5,
            "moved {} world units in x",
            after[0] - before[0]
        );
        assert!(
            (after[1] - before[1] - 30.0 / zoom).abs() < 0.5,
            "moved {} world units in y",
            after[1] - before[1]
        );
        assert!(
            !v.store().has_flag(NodeIx(1), flags::PINNED),
            "other node untouched"
        );
    });
}

#[gpui_kit::test]
fn shift_click_extends_selection_and_empty_click_clears(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| GraphCanvas::new(two_node_store(), cx));
    cx.run_until_parked();
    let (a, b) = view.read_with(cx, |v, _| {
        (
            v.node_window_position(NodeIx(0)),
            v.node_window_position(NodeIx(1)),
        )
    });

    cx.simulate_mouse_down(a, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(a, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_down(b, MouseButton::Left, Modifiers::shift());
    cx.simulate_mouse_up(b, MouseButton::Left, Modifiers::shift());
    view.read_with(cx, |v, _| assert_eq!(v.store().selected().count(), 2));

    let empty = point((a.x + b.x) / 2.0, a.y + px(150.0));
    cx.simulate_mouse_down(empty, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(empty, MouseButton::Left, Modifiers::none());
    view.read_with(cx, |v, _| assert_eq!(v.store().selected().count(), 0));
}

fn render_ms(cx: &mut gpui_kit::VisualTestContext) -> f32 {
    use gpui_kit::test::TestWindowExt as _;
    let t = std::time::Instant::now();
    cx.update(|window, cx| window.render_frame(cx));
    t.elapsed().as_secs_f32() * 1000.0
}

#[gpui_kit::test]
fn scenario_graph_layouts_interaction_during_layout(cx: &mut TestAppContext) {
    use graph_canvas::LayoutScope;
    use graph_layout::{LayoutKind, LayoutOptions};
    cx.executor().allow_parking();
    let store = graph_store::synth::generate(10_000, 20_000, 8, 1);
    let (view, cx) = cx.add_window_view(|_, cx| GraphCanvas::new(store, cx));
    cx.simulate_resize(gpui_kit::size(px(1400.0), px(900.0)));
    cx.run_until_parked();
    render_ms(cx);
    let before = view.read_with(cx, |v, _| v.store().positions().to_vec());
    view.update(cx, |v, cx| {
        v.run_layout(
            LayoutKind::ForceAtlas2,
            LayoutOptions::default(),
            LayoutScope::All,
            false,
            cx,
        )
    });
    let centre = point(px(700.0), px(450.0));
    let mut frames = Vec::new();
    let started = std::time::Instant::now();
    let mut k = 0;
    while started.elapsed() < std::time::Duration::from_millis(1500) {
        // Alternate zooming and panning while the layout streams frames.
        if k % 2 == 0 {
            cx.simulate_event(gpui_kit::ScrollWheelEvent {
                position: centre,
                delta: gpui_kit::ScrollDelta::Pixels(point(
                    px(0.0),
                    px(if k % 4 == 0 { 30.0 } else { -30.0 }),
                )),
                modifiers: Modifiers::none(),
                touch_phase: gpui_kit::TouchPhase::Moved,
            });
        } else {
            let to = point(centre.x + px(3.0), centre.y);
            cx.simulate_mouse_down(centre, MouseButton::Left, Modifiers::none());
            cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
            cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
        }
        k += 1;
        frames.push(render_ms(cx));
        std::thread::sleep(std::time::Duration::from_millis(4));
    }
    let after = view.read_with(cx, |v, _| v.store().positions().to_vec());
    assert_ne!(
        before, after,
        "layout frames were applied during interaction"
    );
    frames.sort_by(f32::total_cmp);
    let p95 = frames[(frames.len() - 1) * 95 / 100];
    eprintln!(
        "{} frames during layout: p50 {:.2} ms, p95 {p95:.2} ms",
        frames.len(),
        frames[frames.len() / 2]
    );
    assert!(
        p95 < 1000.0 / 60.0,
        "frame p95 {p95:.2} ms (60 fps needs < 16.7 ms)"
    );
}

#[gpui_kit::test]
fn stopping_the_layout_freezes_nodes_within_a_frame(cx: &mut TestAppContext) {
    use graph_canvas::LayoutScope;
    use graph_layout::{LayoutKind, LayoutOptions};
    cx.executor().allow_parking();
    let store = graph_store::synth::generate(3_000, 6_000, 4, 2);
    let (view, cx) = cx.add_window_view(|_, cx| GraphCanvas::new(store, cx));
    cx.run_until_parked();
    view.update(cx, |v, cx| {
        v.run_layout(
            LayoutKind::ForceAtlas2,
            LayoutOptions::default(),
            LayoutScope::All,
            false,
            cx,
        )
    });
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        render_ms(cx);
    }
    view.update(cx, |v, cx| v.stop_layout(cx));
    render_ms(cx);
    let frozen = view.read_with(cx, |v, _| v.store().positions().to_vec());
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        render_ms(cx);
    }
    let later = view.read_with(cx, |v, _| v.store().positions().to_vec());
    assert_eq!(frozen, later, "no movement after Stop");
    assert!(view.read_with(cx, |v, _| !v.is_layout_running()));
}

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

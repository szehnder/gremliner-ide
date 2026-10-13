//! Image export. Scenario names map to `openspec/changes/export/specs/export/spec.md`.

#![allow(clippy::unwrap_used)]

use graph_store::export::{Area, ExportStyle, bounds, png, svg};
use graph_store::style::Looks;
use graph_store::{ExtId, GraphStore};

fn style(background: Option<[u8; 3]>) -> ExportStyle {
    ExportStyle {
        palette: vec![[0x4c, 0x78, 0xa8], [0xf5, 0x85, 0x18]],
        edge: [0x99, 0x99, 0x99],
        text: [0x20, 0x20, 0x20],
        background,
        font_family: "sans-serif".into(),
    }
}

/// Ten nodes in a ring, joined in a cycle; every other one has a caption.
fn ring() -> GraphStore {
    let mut g = GraphStore::new();
    let nodes: Vec<_> = (0..10)
        .map(|i| {
            let a = i as f32 / 10.0 * std::f32::consts::TAU;
            let caption = if i % 2 == 0 {
                format!("n{i}")
            } else {
                String::new()
            };
            g.add_node(
                ExtId::Int(i),
                if i < 5 { "a" } else { "b" },
                &caption,
                [a.cos() * 80.0, a.sin() * 80.0],
            )
        })
        .collect();
    for i in 0..10 {
        g.add_edge(
            ExtId::Int(100 + i as i64),
            nodes[i],
            nodes[(i + 1) % 10],
            "e",
        );
    }
    g
}

#[test]
fn scenario_export_svg_contents() {
    let g = ring();
    let out = svg(&g, &Looks::default(), &style(None), Area::Whole);
    assert!(out.starts_with("<svg"));
    assert_eq!(out.matches(r#"class="node""#).count(), 10, "10 node shapes");
    assert_eq!(
        out.matches(r#"<text class="caption""#).count(),
        5,
        "a text per visible caption"
    );
    assert_eq!(out.matches(r#"class="edge""#).count(), 10);
    assert!(out.contains("marker-end=\"url(#arrow0)\""), "arrowheads");
    // It parses as SVG.
    png(&out, 1.0).unwrap();
}

#[test]
fn scenario_export_whole_graph_at_2x() {
    let g = ring();
    let image = svg(
        &g,
        &Looks::default(),
        &style(Some([255, 255, 255])),
        Area::Whole,
    );
    let one = png(&image, 1.0).unwrap();
    let two = png(&image, 2.0).unwrap();
    let decode = |bytes: &[u8]| resvg::tiny_skia::Pixmap::decode_png(bytes).unwrap();
    let (p1, p2) = (decode(&one), decode(&two));
    assert_eq!(
        (p2.width(), p2.height()),
        (p1.width() * 2, p1.height() * 2),
        "twice the resolution"
    );
    // Every node is drawn: the pixel at each node centre is not the white background.
    let (min, _) = bounds(&g).unwrap();
    let origin = [min[0] - 24.0, min[1] - 48.0];
    for (i, [x, y]) in g.positions().iter().enumerate() {
        let px = ((x - origin[0]) * 2.0) as u32;
        let py = ((y - origin[1]) * 2.0) as u32;
        let c = p2.pixel(px, py).unwrap();
        assert!(
            !(c.red() == 255 && c.green() == 255 && c.blue() == 255),
            "node {i} is drawn"
        );
    }
}

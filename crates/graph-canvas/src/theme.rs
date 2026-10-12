//! Canvas colours. This is a theme module (exempt from the design-token check); the app maps its theme
//! tokens onto [`CanvasStyle`] once `app-shell` lands.

use gpui_kit::{Hsla, Pixels, hsla, px};

#[derive(Clone, Debug)]
pub struct CanvasStyle {
    pub background: Hsla,
    /// Categorical palette for vertex labels (12 colours, ADR-0023).
    pub palette: [Hsla; 12],
    pub edge: Hsla,
    pub edge_far: Hsla,
    pub edge_highlight: Hsla,
    pub selection: Hsla,
    pub hover: Hsla,
    pub pinned: Hsla,
    pub label: Hsla,
    pub minimap_bg: Hsla,
    pub minimap_node: Hsla,
    pub minimap_viewport: Hsla,
    pub rubber_band: Hsla,
    pub hud_bg: Hsla,
    pub hud_text: Hsla,
    pub minimap_radius: Pixels,
    pub hud_radius: Pixels,
    /// Icon colour on light node fills, and on dark ones.
    pub icon_on_light: Hsla,
    pub icon_on_dark: Hsla,
}

/// An sRGB byte triple (stylesheet colours) as a GPUI colour.
pub fn color_from_bytes(c: [u8; 3]) -> Hsla {
    gpui_kit::Rgba {
        r: f32::from(c[0]) / 255.0,
        g: f32::from(c[1]) / 255.0,
        b: f32::from(c[2]) / 255.0,
        a: 1.0,
    }
    .into()
}

/// A GPUI colour as an sRGB byte triple.
pub fn bytes_from_color(c: Hsla) -> [u8; 3] {
    let r = gpui_kit::Rgba::from(c);
    let b = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    [b(r.r), b(r.g), b(r.b)]
}

impl CanvasStyle {
    /// The fill colour for vertex label `label` (the legend uses the same).
    pub fn label_color(&self, label: graph_store::LabelIx) -> Hsla {
        self.palette[label.0 as usize % self.palette.len()]
    }

    pub fn light() -> Self {
        let c = |h: f32, s: f32, l: f32| hsla(h / 360.0, s, l, 1.0);
        Self {
            background: c(220.0, 0.20, 0.975),
            palette: [
                c(212.0, 0.75, 0.50),
                c(152.0, 0.55, 0.38),
                c(28.0, 0.90, 0.50),
                c(340.0, 0.65, 0.52),
                c(265.0, 0.55, 0.56),
                c(186.0, 0.70, 0.36),
                c(45.0, 0.90, 0.45),
                c(8.0, 0.70, 0.52),
                c(110.0, 0.45, 0.40),
                c(290.0, 0.40, 0.50),
                c(200.0, 0.30, 0.48),
                c(24.0, 0.45, 0.42),
            ],
            edge: hsla(220.0 / 360.0, 0.20, 0.35, 0.35),
            edge_far: hsla(220.0 / 360.0, 0.20, 0.35, 0.18),
            edge_highlight: c(32.0, 0.95, 0.48),
            selection: c(32.0, 0.95, 0.48),
            hover: hsla(220.0 / 360.0, 0.30, 0.15, 0.85),
            pinned: hsla(220.0 / 360.0, 0.30, 0.15, 0.55),
            label: hsla(220.0 / 360.0, 0.25, 0.20, 0.95),
            minimap_bg: hsla(220.0 / 360.0, 0.20, 0.99, 0.94),
            minimap_node: hsla(212.0 / 360.0, 0.50, 0.45, 0.75),
            minimap_viewport: c(32.0, 0.95, 0.48),
            rubber_band: hsla(212.0 / 360.0, 0.80, 0.50, 0.12),
            hud_bg: hsla(0.0, 0.0, 0.0, 0.6),
            hud_text: hsla(120.0 / 360.0, 0.6, 0.75, 1.0),
            minimap_radius: px(6.0),
            hud_radius: px(4.0),
            icon_on_light: hsla(0.0, 0.0, 0.1, 0.9),
            icon_on_dark: hsla(0.0, 0.0, 1.0, 0.95),
        }
    }

    pub fn dark() -> Self {
        let c = |h: f32, s: f32, l: f32| hsla(h / 360.0, s, l, 1.0);
        Self {
            background: c(222.0, 0.16, 0.10),
            palette: [
                c(212.0, 0.80, 0.62),
                c(152.0, 0.55, 0.52),
                c(32.0, 0.90, 0.60),
                c(340.0, 0.70, 0.64),
                c(265.0, 0.65, 0.68),
                c(186.0, 0.65, 0.50),
                c(50.0, 0.85, 0.58),
                c(8.0, 0.75, 0.62),
                c(110.0, 0.45, 0.55),
                c(290.0, 0.45, 0.62),
                c(200.0, 0.30, 0.65),
                c(24.0, 0.45, 0.55),
            ],
            edge: hsla(220.0 / 360.0, 0.15, 0.70, 0.35),
            edge_far: hsla(220.0 / 360.0, 0.15, 0.70, 0.18),
            edge_highlight: c(42.0, 0.95, 0.62),
            selection: c(42.0, 0.95, 0.62),
            hover: hsla(0.0, 0.0, 1.0, 0.9),
            pinned: hsla(0.0, 0.0, 1.0, 0.55),
            label: hsla(220.0 / 360.0, 0.20, 0.88, 0.92),
            minimap_bg: hsla(222.0 / 360.0, 0.18, 0.14, 0.92),
            minimap_node: hsla(212.0 / 360.0, 0.50, 0.70, 0.75),
            minimap_viewport: c(42.0, 0.95, 0.62),
            rubber_band: hsla(212.0 / 360.0, 0.80, 0.62, 0.15),
            hud_bg: hsla(0.0, 0.0, 0.0, 0.6),
            hud_text: hsla(120.0 / 360.0, 0.6, 0.75, 1.0),
            minimap_radius: px(6.0),
            hud_radius: px(4.0),
            icon_on_light: hsla(0.0, 0.0, 0.1, 0.9),
            icon_on_dark: hsla(0.0, 0.0, 1.0, 0.95),
        }
    }
}

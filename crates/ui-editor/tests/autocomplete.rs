//! Autocomplete in the real editor: typing opens the completion menu, Enter accepts and places the
//! cursor, signature help follows the cursor, and schema warnings offer quick fixes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, WindowOptions};
use schema::Schema;
use ui_editor::QueryEditor;

fn air() -> Arc<Schema> {
    Arc::new(
        Schema::from_json(include_str!(
            "../../gremlin-syntax/tests/fixtures/air-routes-schema.json"
        ))
        .unwrap(),
    )
}

struct Fixture {
    handle: AnyWindowHandle,
    editor: Entity<QueryEditor>,
}

impl Fixture {
    fn new(cx: &mut TestAppContext, schema: Option<Arc<Schema>>) -> Self {
        cx.update(|cx| {
            gpui_kit::init(cx);
            ui_editor::register();
        });
        let (handle, editor): (AnyWindowHandle, Entity<QueryEditor>) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| QueryEditor::new("", window, cx))
                })
            })
            .unwrap();
        let f = Self { handle, editor };
        f.with(cx, |e, window, cx| {
            e.set_schema(schema, cx);
            e.focus(window, cx);
        });
        f.settle(cx);
        f
    }

    fn with<R>(
        &self,
        cx: &mut TestAppContext,
        f: impl FnOnce(
            &mut QueryEditor,
            &mut gpui_kit::Window,
            &mut gpui_kit::Context<'_, QueryEditor>,
        ) -> R,
    ) -> R {
        let editor = self.editor.clone();
        cx.update_window(self.handle, |_, window, cx| {
            editor.update(cx, |e, cx| f(e, window, cx))
        })
        .unwrap()
    }

    fn settle(&self, cx: &mut TestAppContext) {
        cx.run_until_parked();
        cx.update_window(self.handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.run_until_parked();
    }

    fn input(&self, text: &str, cx: &mut TestAppContext) {
        for ch in text.chars() {
            cx.update_window(self.handle, |_, window, cx| {
                window.input(&ch.to_string(), cx)
            })
            .unwrap();
            self.settle(cx);
        }
    }

    fn press(&self, key: &str, cx: &mut TestAppContext) {
        cx.update_window(self.handle, |_, window, cx| window.press(key, cx))
            .unwrap();
        self.settle(cx);
    }

    fn text(&self, cx: &mut TestAppContext) -> String {
        self.with(cx, |e, _, cx| e.text(cx))
    }

    fn cursor(&self, cx: &mut TestAppContext) -> usize {
        self.with(cx, |e, _, cx| e.cursor(cx))
    }

    fn menu(&self, cx: &mut TestAppContext) -> (bool, Vec<String>) {
        self.with(cx, |e, _, cx| {
            let state = e.state().read(cx).completion_menu_state();
            (
                state.open,
                state.items.iter().map(|i| i.label.clone()).collect(),
            )
        })
    }
}

#[gpui_kit::test]
fn typing_a_step_prefix_opens_the_menu_and_enter_inserts_it(cx: &mut TestAppContext) {
    let f = Fixture::new(cx, None);
    f.input("g.V().hasL", cx);
    let (open, items) = f.menu(cx);
    assert!(open, "the menu opens while typing");
    assert_eq!(items.first().map(String::as_str), Some("hasLabel"));
    f.press("enter", cx);
    assert_eq!(f.text(cx), "g.V().hasLabel()");
    assert_eq!(
        f.cursor(cx),
        "g.V().hasLabel(".len(),
        "cursor inside the parentheses"
    );
}

#[gpui_kit::test]
fn labels_complete_inside_strings_with_a_schema(cx: &mut TestAppContext) {
    let f = Fixture::new(cx, Some(air()));
    f.input("g.V().hasLabel('air", cx);
    let (open, items) = f.menu(cx);
    assert!(open);
    assert_eq!(items, ["airport"]);
    f.press("enter", cx);
    // The editor auto-closed the quote and parenthesis; the completion fills in between.
    assert_eq!(f.text(cx), "g.V().hasLabel('airport')");
    // Signature help follows the cursor into the call.
    f.with(cx, |e, window, cx| {
        let text = "g.V().hasLabel('airport').has('airport', ";
        e.set_text(text, window, cx);
        e.state()
            .update(cx, |s, cx| s.set_selected_range(text.len()..text.len(), cx));
    });
    f.settle(cx);
    let help = f
        .with(cx, |e, _, _| e.signature_help().cloned())
        .expect("signature help");
    assert_eq!(help.active_param_text(), Some("String propertyKey"));
    f.press("escape", cx);
}

#[gpui_kit::test]
fn misspelled_label_warns_and_quick_fix_replaces_it(cx: &mut TestAppContext) {
    let f = Fixture::new(cx, Some(air()));
    f.with(cx, |e, window, cx| {
        e.set_text("g.V().hasLabel('airprt')", window, cx)
    });
    f.settle(cx);
    let warnings: Vec<String> = f.with(cx, |e, _, _| {
        e.diagnostics().iter().map(|d| d.message.clone()).collect()
    });
    assert!(
        warnings
            .iter()
            .any(|m| m.contains("Unknown vertex label `airprt`")),
        "{warnings:?}"
    );
    let at = "g.V().hasLabel('air".len();
    let fixes = f.with(cx, |e, _, _| e.quick_fixes_at(at));
    assert_eq!(fixes[0].0, "Replace with `airport`");
    assert!(f.with(cx, |e, window, cx| e.apply_quick_fix(at, window, cx)));
    f.settle(cx);
    assert_eq!(f.text(cx), "g.V().hasLabel('airport')");
    let warnings = f.with(cx, |e, _, _| e.diagnostics().len());
    assert_eq!(warnings, 0, "fixed");
    // Without a schema there are no schema warnings.
    f.with(cx, |e, window, cx| {
        e.set_schema(None, cx);
        e.set_text("g.V().hasLabel('airprt')", window, cx);
    });
    assert!(f.with(cx, |e, _, _| e.diagnostics().is_empty()));
}

#[gpui_kit::test]
fn quick_fix_through_the_code_action_menu(cx: &mut TestAppContext) {
    let f = Fixture::new(cx, Some(air()));
    f.with(cx, |e, window, cx| {
        e.set_text("g.V().out('rout')", window, cx);
        let at = "g.V().out('ro".len();
        e.state().update(cx, |s, cx| {
            let pos = gpui_kit::component::input::RopeExt::offset_to_position(s.text(), at);
            s.set_cursor_position(pos, window, cx);
        });
    });
    f.settle(cx);
    f.press("ctrl-.", cx);
    f.press("enter", cx);
    assert_eq!(f.text(cx), "g.V().out('route')");
}

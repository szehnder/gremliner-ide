//! Helpers shared by the UI test files.

use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{SharedString, VisualTestContext};

/// Click `id` once it has stopped moving. Dialogs slide in over real time, so a click computed from a
/// mid-animation frame can miss its target.
pub fn click_settled(cx: &mut VisualTestContext, id: &str) {
    let id = SharedString::from(id.to_owned());
    let mut last = None;
    for _ in 0..200 {
        cx.run_until_parked();
        let bounds = cx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find(id.clone()).map(|e| e.bounds())
        });
        if bounds.is_some() && bounds == last {
            break;
        }
        last = bounds;
        std::thread::sleep(Duration::from_millis(15));
    }
    cx.update(|window, cx| window.click(id, cx));
}

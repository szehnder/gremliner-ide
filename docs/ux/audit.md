# UX audit

The polish-pass checklist (openspec `ux-quality`, ADR-0023). One row per surface. Re-check a row when its
surface changes. Screenshots come from `cargo xtask screenshot --all-surfaces`, which writes
`target/screenshots/gallery/index.html`. The gallery name of each surface is in the last column.

Legend: ✓ meets the bar · — not applicable · ⚠ known gap (see below).

**Columns**
- **Tokens:** no raw colours or sizes. `cargo xtask ci` greps for them.
- **Empty:** a designed empty state with a next action.
- **Loading:** a spinner or progress for operations over 150 ms.
- **Error + recovery:** the reason and a way forward.
- **Keyboard:** reachable, and focus is visible.
- **A11y:** roles and names. `scenario_ux_quality_accessible_names` checks this.

| Surface | Tokens | Empty | Loading | Error + recovery | Keyboard | A11y | Gallery |
|---|---|---|---|---|---|---|---|
| Title bar (sidebar, results, inspector, settings toggles; command centre) | ✓ | — | — | — | ✓ toggles have shortcuts | ✓ | `welcome` |
| Welcome view (empty editor area) | ✓ | ✓ Add connection, palette, shortcuts | — | — | ✓ | ✓ | `welcome` |
| First-run tour and sample guide | ✓ | — | — | ✓ save failure → notification | ✓ Tab between Skip, Sample, Next | ✓ | `first-launch` |
| Connections sidebar | ✓ | ✓ "No connections" + Add | ✓ connecting dot | ✓ reason, suggestion, **Retry**, **Edit connection** | ✓ rows are tab stops, Enter connects | ✓ rows named "name, endpoint, status" | `connection-error` |
| Connection form | ✓ | — | ✓ Test / Save spinners | ✓ field errors, test result | ✓ Enter saves anywhere in the form | ✓ | `new-connection` |
| Command palette | ✓ | ✓ no matches | — | — | ✓ | ✓ | `palette` |
| Keyboard shortcuts sheet | ✓ | — | — | — | ✓ Ctrl/Cmd+/ | ✓ | `shortcuts` |
| Settings (General, AI) | ✓ | ✓ no LLM profiles | ✓ model list, test | ✓ test errors | ✓ | ✓ | `settings` |
| Query tab (editor, toolbar, parameters, prompt bar) | ✓ | — | ✓ running state, prompt bar steps | ✓ inline diagnostics | ✓ every toolbar action has a key | ✓ editor named "Gremlin query" | `query-tab` |
| Results dock | ✓ | ✓ "No results yet" + run shortcut | ✓ streaming count, Cancel (Esc) | ✓ message, **Run again**, **Fix with assistant** | ✓ | ✓ | `query-tab`, `query-error` |
| Graph view | ✓ | — | ✓ "Fetching…", layout progress | ✓ follow-up errors → notification | ✓ canvas keys (F, E, N, Delete…) | ✓ selection summary on `graph-stats` | `graph` |
| Table / JSON / Console views | ✓ | — | ✓ (streamed) | — (errors shown by the dock) | ✓ | ✓ | `table` |
| Profile / Explain views | ✓ | — | ✓ | — (errors shown by the dock) | ✓ | ✓ | — (needs Profile run) |
| Debugger strip | ✓ | — | ✓ per-step spinners | ✓ unsupported-step notes | ✓ F9–F11, Ctrl+F5, Shift+F5 | ✓ | — (needs a debug session) |
| Inspector | ✓ | ✓ nothing selected | — | ✓ edit errors, previews | ✓ | ✓ | `graph` |
| Assistant panel | ✓ | ✓ "No query tab" | ✓ streaming turn | ✓ provider errors | ✓ | ✓ | `assistant` |
| Schema graph | ✓ | ✓ "No schema yet" | ✓ "Discovering the schema…" | ✓ Refresh | ✓ | ✓ | `schema-graph` |
| Graph explorer | ✓ | ✓ start hint | ✓ | ✓ run errors in results | ✓ | ✓ | `explorer` |
| History | ✓ | ✓ empty / no matches | — (local) | — | ✓ | ✓ | `history` |
| Saved queries | ✓ | ✓ empty + how to save | — (local) | — (local writes; failures are logged) | ✓ | ✓ | `saved` |
| Report tab | ✓ | — | ✓ | ✓ | ✓ | ✓ | — |
| Status bar | ✓ | — | ✓ background activity | — | — | ✓ | all |

## Motion

All transitions use `ui_theme::Motion` (120 / 200 / 320 ms, ease-out). gpui-kit's dialogs, popovers,
notifications and spinners honour `App::reduce_motion`. The graph camera (focus and fit) honours it in
`graph-canvas`, and inertial panning stops too.

Settings → General → Motion has three choices:
- **System** (default): follow the OS preference, which `ui_workspace::motion` reads at startup. That is KDE's
  `AnimationDurationFactor`, GNOME's `enable-animations`, or macOS's Reduce motion. On Windows nothing is read,
  so System means full motion.
- **Reduce:** transitions are instant.
- **Full:** always animate.

## Known gaps

- ⚠ **Dock tab strip names.** gpui-kit 0.7.1's dock draws the panel tabs, the "…" tab menu, the close-tab
  buttons and the dock-collapse toggles without accessible names, and it exposes no way to add them. The
  accessibility test excludes exactly these elements (`dock_chrome` in `crates/ui-workspace/tests/ux.rs`).
  Fix this upstream, or with a patched gpui-component, then remove the exclusion.
- ⚠ **Live screenshot state.** Windows launched from a script get only their first frames on the Wayland
  dev machine (see `docs/gpui-notes.md`), so connected surfaces can show their loading state rather than
  the result.
- Visual review in both themes is the maintainer's gate (polish-pass task 5.3).

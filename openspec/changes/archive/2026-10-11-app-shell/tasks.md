# Tasks

## 1. Foundations

- [x] 1.1 Create the global tokio runtime and `spawn_io` bridge in `ui-workspace::io`; verify with a headless test that a 200 ms async sleep completes without blocking a concurrent foreground task
- [x] 1.2 Add the `storage` SQLite bootstrap (WAL, migrations, `settings` and `ui_state` tables, dedicated thread); verify `cargo test -p storage` round-trips a setting
- [x] 1.3 Embed fonts and icons via `AssetSource`; verify `scenario_theming_fonts_available_offline` (text system reports Inter/JetBrains Mono loaded)

## 2. Theming

- [x] 2.1 Implement `Tokens` with light/dark JSON themes and System mode following OS appearance; verify `scenario_theming_switch_to_dark` and `scenario_theming_follow_system_appearance`
- [x] 2.2 Add the WCAG contrast unit test; verify `scenario_theming_contrast_check`
- [x] 2.3 Implement UI/editor font size settings and zoom actions; verify `scenario_theming_zoom_in`

## 3. Shell

- [x] 3.1 Build the dock layout (sidebar, editor tabs, results dock, right panel, status bar) with `WorkspacePanel` registration; verify `scenario_app_shell_toggle_the_results_dock` and `scenario_app_shell_resize_the_sidebar`
- [x] 3.2 Persist and restore window and dock state; verify `scenario_app_shell_restart_restores_layout` and `scenario_app_shell_corrupt_layout_falls_back_to_default`
- [x] 3.3 Implement the welcome view; verify `scenario_app_shell_first_launch`
- [x] 3.4 Status bar activity indicator wired to `spawn_io`; verify `scenario_app_shell_slow_background_task`
- [x] 3.5 CLI flags `--perf-hud`, `--exit-after`, `--open-connection`, `--run`, `--theme` (later flags accepted as no-ops); verify `scenario_app_shell_exit_after_timeout`

## 4. Command palette and settings

- [x] 4.1 Action registry with `CommandMeta` and the palette UI with fuzzy matching; verify `scenario_command_palette_fuzzy_match` and `scenario_command_palette_run_an_action`
- [x] 4.2 Recent-first ordering and `when` predicates; verify `scenario_command_palette_recent_ordering` and `scenario_command_palette_no_editor_open`
- [x] 4.3 Settings view with General page, immediate apply, reset; verify `scenario_settings_open_settings`, `scenario_settings_persist_a_change`, `scenario_settings_reset_general_settings`

## 5. Screenshot tooling

- [x] 5.1 Implement `cargo xtask screenshot`; verify it writes a PNG of the welcome view, and record what you learned about GPUI in `docs/gpui-notes.md`

## 6. Verification

- [x] 6.1 `cargo xtask ci` is green
- [x] 6.2 `openspec validate app-shell --strict` passes
- [x] 6.3 Screenshot review of the welcome view in light and dark themes

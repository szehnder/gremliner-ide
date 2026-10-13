# Tasks

## 1. Audit tooling

- [x] 1.1 Write `docs/ux/audit.md` and `cargo xtask screenshot --all-surfaces` gallery; verify the gallery renders all surfaces

## 2. States and motion

- [x] 2.1 Empty/loading/error states across panels; verify `scenario_ux_quality_empty_results_dock` and `scenario_ux_quality_error_with_recovery`
- [x] 2.2 Motion helper and reduce-motion; verify `scenario_ux_quality_reduce_motion`

## 3. Keyboard and onboarding

- [x] 3.1 Shortcuts sheet and focus visibility; verify `scenario_ux_quality_shortcuts_sheet` and `scenario_ux_quality_keyboard_only_connection`
- [x] 3.2 First-run tour and sample connection guide; verify `scenario_ux_quality_skip_tour`

## 4. Accessibility

- [x] 4.1 Accessible names and graph selection summary; verify `scenario_ux_quality_accessible_names`

## 5. Verification

- [x] 5.1 `cargo xtask ci` is green
- [x] 5.2 `openspec validate polish-pass --strict` passes
- [ ] 5.3 Maintainer review of the screenshot gallery in both themes (pending: `cargo xtask screenshot --all-surfaces`, then open `target/screenshots/gallery/index.html`)

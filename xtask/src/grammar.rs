//! `cargo xtask grammar [--test]`: regenerate the tree-sitter parser from `grammar.js` (and run the
//! grammar's own corpus tests). Needs Node; `npx` fetches the pinned tree-sitter CLI. Building the crate
//! never needs this: the generated C is checked in.

use std::path::Path;
use std::process::Command;

use anyhow::Result;

/// Must match the `tree-sitter` runtime version gpui-kit uses (see Cargo.lock).
const CLI: &str = "tree-sitter-cli@0.26.13";

pub fn run(root: &Path, test: bool) -> Result<()> {
    let dir = root.join("crates/gremlin-syntax/grammar");
    crate::run(
        "tree-sitter generate",
        Command::new("npx")
            .args(["--yes", CLI, "generate"])
            .current_dir(&dir),
    )?;
    if test {
        crate::run(
            "tree-sitter test",
            Command::new("npx")
                .args(["--yes", CLI, "test"])
                .current_dir(&dir),
        )?;
    }
    Ok(())
}

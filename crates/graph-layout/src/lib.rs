//! Graph layout algorithms run off the main thread (ADR-0009, ADR-0010).
//!
//! See `AGENTS.md` for crate layering rules and `openspec/ROADMAP.md` for the change that fills this crate in.

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {
        assert_eq!(env!("CARGO_PKG_NAME"), "graph-layout");
    }
}

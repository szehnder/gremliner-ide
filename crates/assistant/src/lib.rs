//! Schema-grounded NL-to-Gremlin pipeline and assistant modes (ADR-0019).
//!
//! See `AGENTS.md` for crate layering rules and `openspec/ROADMAP.md` for the change that fills this crate in.

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {
        assert_eq!(env!("CARGO_PKG_NAME"), "assistant");
    }
}

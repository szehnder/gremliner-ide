//! Compiles the checked-in tree-sitter parser (`grammar/src/parser.c`). No Node or tree-sitter CLI is
//! needed to build; `cargo xtask grammar` regenerates the C from `grammar/grammar.js`.

fn main() {
    let src = std::path::Path::new("grammar/src");
    cc::Build::new()
        .include(src)
        .file(src.join("parser.c"))
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .flag_if_supported("-Wno-trigraphs")
        .compile("tree-sitter-gremlin");
    println!("cargo:rerun-if-changed=grammar/src/parser.c");
    println!("cargo:rerun-if-changed=grammar/src/tree_sitter/parser.h");
}

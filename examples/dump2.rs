// like dump, but with byte ranges
fn main() {
    let src = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_c_sharp::LANGUAGE.into()).unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let mut cursor = tree.walk();
    let mut depth = 0;
    loop {
        let n = cursor.node();
        println!("{}{}{} [{}..{}]{}", "  ".repeat(depth), if n.is_named() { "" } else { "~" }, n.kind(), n.start_byte(), n.end_byte(), if n.child_count() == 0 { format!(" {:?}", &src[n.byte_range()]) } else { String::new() });
        if cursor.goto_first_child() { depth += 1; continue; }
        loop {
            if cursor.goto_next_sibling() { break; }
            if !cursor.goto_parent() { return; }
            depth -= 1;
        }
    }
}

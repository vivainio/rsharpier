use std::io::Read;
fn main() {
    let mut src = String::new();
    match std::env::args().nth(1) {
        Some(p) => src = std::fs::read_to_string(p).unwrap(),
        None => { std::io::stdin().read_to_string(&mut src).unwrap(); }
    }
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_c_sharp::LANGUAGE.into()).unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let mut cursor = tree.walk();
    let mut depth = 0;
    loop {
        let n = cursor.node();
        let named = if n.is_named() { "" } else { "~" };
        let text: String = if n.child_count() == 0 { format!(" {:?}", &src[n.byte_range()]) } else { String::new() };
        let field = cursor.field_name().map(|f| format!("{f}: ")).unwrap_or_default();
        println!("{}{}{}{}{}{}", "  ".repeat(depth), field, named, n.kind(), if n.is_error()||n.is_missing() {" !!ERR"} else {""}, text);
        if cursor.goto_first_child() { depth += 1; continue; }
        loop {
            if cursor.goto_next_sibling() { break; }
            if !cursor.goto_parent() { return; }
            depth -= 1;
        }
    }
}

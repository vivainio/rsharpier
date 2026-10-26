fn main() {
    let src = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_c_sharp::LANGUAGE.into()).unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let protected = csharpier_rs::trivia::protected_ranges(tree.root_node());
    let pre = csharpier_rs::trivia::preprocess(&src, &protected, &[]);
    println!("{:?}", pre.events);
    println!("{:?}", csharpier_rs::symbols::get_sets(&pre.events));
}

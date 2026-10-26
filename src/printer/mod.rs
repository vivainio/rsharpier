//! Port of CSharpier.Core/CSharp/SyntaxPrinter on top of tree-sitter nodes.

mod chain;
mod exprs;
mod members;
mod patterns;
mod stmts;
mod nodes;
mod token;

use tree_sitter::Node;

use crate::doc::Doc;
use crate::trivia::{Tokens, Trivia};

pub use token::*;

#[derive(Default)]
pub struct State {
    pub next_trivia_needs_line: bool,
    pub reordered_modifiers: bool,
    pub moved_trailing_trivia: bool,
    /// (token index of the item with the trailing comment, printed comma): a trailing comma that
    /// has to be printed before a trailing comment
    pub trailing_comma: Option<(usize, Doc)>,
}

pub struct Ctx<'a> {
    pub src: &'a str,
    pub tokens: &'a Tokens,
    pub state: State,
    /// node kinds we have no printer for yet, they are printed verbatim
    pub unhandled: Vec<String>,
}

impl<'a> Ctx<'a> {
    pub fn text(&self, node: Node) -> &'a str {
        &self.src[node.byte_range()]
    }

    pub fn first_token_index(&self, node: Node) -> usize {
        self.tokens.first_at_or_after(node.start_byte())
    }

    pub fn last_token_index(&self, node: Node) -> usize {
        self.tokens
            .last_ending_at_or_before(node.end_byte())
            .unwrap_or(0)
    }

    pub fn leading_trivia(&self, node: Node) -> &'a [Trivia] {
        &self.tokens.list[self.first_token_index(node)].leading
    }

    pub fn trailing_trivia(&self, node: Node) -> &'a [Trivia] {
        &self.tokens.list[self.last_token_index(node)].trailing
    }
}

/// children, without comments (which are trivia as far as the printers are concerned)
pub fn children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.kind() != "comment")
        .collect()
}

pub fn named_children(node: Node) -> Vec<Node> {
    children(node).into_iter().filter(|c| c.is_named()).collect()
}

pub fn children_of_kind<'t>(node: Node<'t>, kind: &str) -> Vec<Node<'t>> {
    children(node).into_iter().filter(|c| c.kind() == kind).collect()
}

pub fn child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    children(node).into_iter().find(|c| c.kind() == kind)
}

pub fn field<'t>(node: Node<'t>, name: &str) -> Option<Node<'t>> {
    node.child_by_field_name(name)
}

/// the modifier keywords of a declaration
pub fn modifiers<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    children_of_kind(node, "modifier")
}

pub fn extra_new_lines_for_trivia(trivia: &[Trivia]) -> Doc {
    for t in trivia {
        if t.kind == crate::trivia::TriviaKind::EndOfLine {
            // ensures we only print a single new line
            return Doc::hard_line();
        }
        if t.kind != crate::trivia::TriviaKind::Whitespace {
            return Doc::Null;
        }
    }
    Doc::Null
}

pub fn extra_new_lines(ctx: &Ctx, node: Node) -> Doc {
    if node.parent().map(|p| p.kind()) == Some("global_statement") {
        return Doc::Null;
    }
    extra_new_lines_for_trivia(ctx.leading_trivia(node))
}

pub fn print_root(ctx: &mut Ctx, root: Node) -> Doc {
    nodes::print(ctx, root)
}

pub use nodes::print as print_node;

pub mod doc;
pub mod api;
pub mod cli;
pub mod format;
pub mod options;
pub mod printer;
pub mod symbols;
pub mod trivia;
pub mod xml;

pub use format::{format, preprocessor_symbol_sets, FormatError, FormatResult};

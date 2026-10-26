pub mod doc;
pub mod cli;
pub mod format;
pub mod options;
pub mod printer;
pub mod symbols;
pub mod trivia;
pub mod xml;

pub use format::{format, FormatError, FormatResult};

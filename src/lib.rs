//! Komainu: checked, macro-free Markdown from a template and symbol/expression definitions.

mod diagnostic;
mod document;
mod expr;
mod name;
mod parser;
mod template;

pub use diagnostic::{Diagnostic, Error, Location};
pub use document::Document;
pub use expr::Expr;

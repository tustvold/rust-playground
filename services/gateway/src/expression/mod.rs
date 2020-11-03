use calculator_client::{ComputeOperation, ComputeValue};
pub use parser::{parse, ParseError};

mod parser;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Constant(ComputeValue),
    Application(ComputeOperation, Box<Expr>, Box<Expr>),
}

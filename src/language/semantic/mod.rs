//! Semantic stages: the checker orchestrates traversal while `types` owns
//! the language type vocabulary, symbols and assignability rules.

mod checker;
mod types;

pub(crate) use checker::check;

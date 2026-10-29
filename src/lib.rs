//! Seloryn's open language and runtime core.
//!
//! The library owns the source pipeline, interpreter, compute engine, jobs,
//! project contracts and core LSP. The command-line binary is intentionally a
//! thin adapter over [`cli::run`]. Product delivery code (installer, editor
//! client and font tooling) is not part of this crate.

pub mod cli;

pub(crate) mod builtins;
pub(crate) mod compute;
pub(crate) mod data;
pub(crate) mod diagnostic;
pub(crate) mod jobs;
pub(crate) mod json;
pub(crate) mod language;
pub(crate) mod project;
pub(crate) mod protocols;
pub(crate) mod runtime;

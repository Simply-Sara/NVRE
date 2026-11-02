//! `nvre-script` — NVRE's own obscript toolchain.
//!
//! obscript is the language the GECK compiles: a `SCPT` record's `SCTX`
//! subrecord holds its source and its `SCDA` holds the bytecode the game runs.
//! This crate turns the first into the second, and is the reason NVRE does not
//! have to shell out to the GECK (a Windows GUI with no script-compile command
//! line) or to Wine to compile a script.
//!
//! # The modules
//!
//! - [`lex`]: source text to tokens, never failing.
//! - [`opcodes`]: the statement and command opcode tables, and the event
//!   codes a `Begin` block opens with.
//! - [`compile`]: source text to compiled bytecode, the whole point of the
//!   crate.
//!
//! # How this is validated
//!
//! Against the game itself, not against a spec written from memory. Every rule
//! in the emitter is pinned by compiling the source of a shipped unit and
//! comparing the result with the bytes the game wrote for it: `FalloutNV.esm`
//! carries 2,576 `SCPT` records with both an `SCTX` and an `SCDA`, so each one
//! is a source→bytecode pair the compiler can be checked against. The
//! `parity` example reports how many of them the compiler reproduces exactly,
//! and that number is the acceptance test for every change here.
//!
//! The byte format this crate reads and writes is documented in
//! `docs/obscript-bytecode.md`, which is NVRE's own, derived from the game's
//! own bytes.

pub mod compile;
pub mod lex;
pub mod opcodes;
pub mod parse;

/// The crate's own error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The source could not be parsed into statements.
    #[error("{0}")]
    Parse(String),
}

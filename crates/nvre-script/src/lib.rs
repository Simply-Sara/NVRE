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
//! - [`emit`]: the instruction framing — `[opcode:u16][argLen:u16][args]` —
//!   and the reader that takes one apart again.
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
//! `parity_all` example reports how many of them the compiler reproduces
//! exactly, and that number is the acceptance test for every change here.
//!
//! Measured over the masters **and their DLCs**, compiling each plugin with its
//! own `MAST` masters loaded:
//!
//! ```text
//! units:      5,685
//! byte-equal: 5,681 / 5,681   (100.00% of the units that compile)
//! refused:        3             the source names data the record no longer carries
//! differs:        4             two undetermined, two stale
//! ```
//!
//! `FalloutNV.esm` is 2,576 / 2,576 and `Fallout3.esm` 1,257 / 1,257; every
//! other plugin is 100% of its compileable units.
//!
//! Four units compile to different bytes and three are refused, and none of the
//! seven is an emitter defect — each is a case where the shipped `.pex` no
//! longer agrees with the shipped source or record:
//!
//! - **Three are refused because the source names a form the record does not
//!   contain**, under a spelling the form no longer has: `HologramStopPackage`
//!   (only `NVDLC01HologramStopPackage` exists, 01002E23),
//!   `NVDLC02SorrowMQ05REF` (that slot now holds `NVDLC02SorrowMQ05a1REF`) and
//!   `NVDLC03SinkNukaColaMachineMsg` (now `NVDLC03HQNukaColaMachineMsg`,
//!   0100D590). `Anchorage.esm` and `Zeta.esm` add three more of the same kind.
//!   The bytecode holds the FormID the GECK resolved at compile time; the EDIDs
//!   were renamed afterwards. The GECK itself could not recompile these today,
//!   so they are reported and counted apart rather than folded into parity.
//! - **One is a stale variable id**: `NVDLC04TunnelDeathclawRushTriggerSCRIPT`
//!   writes `73 08 00` for `bRetreat` of `NVDLC04TunnelerSCRIPT`, whose record
//!   now puts that name at id 9 — the record was re-saved after the `.pex` was
//!   built.
//! - **Two are a duplicate reference slot with no recoverable answer**: a record
//!   holding the same FormID twice (`00000014`, `Player`) in two slots stores
//!   nothing that says which spelling took which slot, and the eight such units
//!   in the corpus split three ways, so no rule is determined by the data.
//! - **One is a compiler-version difference**: `GetInCharGen` is `58 7F 12 00
//!   00` in four units and `58 7C 12 02 00 00 00` in one, with nothing in the
//!   source to choose between them.
//!
//! The byte format this crate reads and writes is documented in
//! `docs/obscript-bytecode.md`, which is NVRE's own, derived from the game's
//! own bytes.

pub mod compile;
pub mod emit;
pub mod lex;
pub mod opcodes;
pub mod parse;
pub mod value;

pub use compile::{compile, compile_with_refs};
pub use parse::{Decl, DeclKind, RefEntry, RefKind, RefSlot, RemoteScript, Script, Stmt, VarEntry};

/// The crate's own error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The source could not be parsed into statements.
    #[error("{0}")]
    Parse(String),
}

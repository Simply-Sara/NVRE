//! `nvre-papyrus` — NVRE's Papyrus toolchain.
//!
//! Papyrus is the scripting language Bethesda's Creation Engine games run:
//! Skyrim, Fallout 4, Fallout 76 and Starfield. A script is written as `.psc`
//! source and shipped as a `.pex` binary the engine loads and executes, so
//! this crate has three layers and they are separable on purpose:
//!
//! - [`pex`] — the **container**: header, string table, debug info, user
//!   flags, objects, variables, properties, states, functions and
//!   instructions. Reads and writes.
//! - [`opcodes`] — the **instruction set**: what each opcode is called and how
//!   many operands it takes.
//! - [`compile`] — `.psc` source to a `.pex`, the whole point of the crate.
//!   (Not written yet; see the crate's own roadmap note below.)
//!
//! # Endianness is not a detail
//!
//! **Skyrim's `.pex` is big-endian; Fallout 4's is little-endian.** The two
//! carry the same magic number written in their own byte order, so the first
//! four bytes decide which one a file is — `FA 57 C0 DE` is Skyrim,
//! `DE C0 57 FA` is Fallout 4. A reader that assumes one byte order produces
//! string lengths in the thousands and a string table that runs off the end of
//! the file. [`pex::Endianness`] is therefore part of the parsed file rather
//! than a global.
//!
//! Measured on the shipped corpus: every `.pex` in `Skyrim - Misc.bsa`
//! (14,026 files) is `FA 57 C0 DE`, version 3.1 or 3.2, game id 1.
//!
//! # Where the layout comes from
//!
//! From the game's own bytes, the same way [`nvre_script`](../nvre_script)
//! treats obscript: a reference parse of the whole shipped corpus, then the
//! Rust written against what that parse settled. `tools/pexref.py` is that
//! reference parse, and it consumes every one of those files to the last byte.
//!
//! Two details are worth naming because they are where a guess goes wrong:
//!
//! - **The value tag order is not the order the community pages list.** The
//!   tag after an operand is `0 none, 1 identifier, 2 string, 3 int, 4 float,
//!   5 bool`. Reading `1` as a boolean eats a byte and shifts everything
//!   after it.
//! - **Fallout 4's object record inserts fields Skyrim's does not have**, so a
//!   single reader cannot treat the two as the same layout with different byte
//!   order. [`pex`] keeps the game flavour explicit for that reason.
//!
//! # Status
//!
//! The container reader is done and verified against the shipped corpus. The
//! writer round-trips a parsed file byte for byte. The compiler is the next
//! layer and is not built yet: the crate exposes [`compile`] as the place it
//! will live, and refuses rather than pretending.

pub mod lex;
pub mod opcodes;
pub mod parse;
pub mod pex;

pub use lex::lex;
pub use parse::parse;
pub use pex::{Endianness, Flavour, Instruction, Object, PexFile, Value};

/// The crate's own error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The bytes are not a Papyrus script.
    #[error("not a Papyrus script: {0}")]
    NotPex(String),
    /// A read ran past the end of the file.
    #[error("truncated .pex: {0}")]
    Truncated(String),
    /// A field holds a value the format does not allow.
    #[error("invalid .pex structure: {0}")]
    Layout(String),
    /// A string index points outside the string table.
    #[error("string index {index} is outside the {len}-entry string table")]
    BadStringIndex {
        /// The index that was read.
        index: u16,
        /// How many strings the table actually holds.
        len: usize,
    },
    /// A value tag the format does not define.
    #[error("value type {0} is not one of the six the format defines")]
    BadValueType(u8),
    /// An opcode the table does not name.
    #[error("opcode {0:#04x} is not in the instruction table")]
    BadOpcode(u8),
    /// The file is one this crate does not read yet.
    #[error("unsupported .pex: {0}")]
    Unsupported(String),
    /// Source text could not be parsed.
    #[error("{0}")]
    Parse(String),
}

impl From<nvre_util::OutOfBounds> for Error {
    fn from(e: nvre_util::OutOfBounds) -> Self {
        Error::Truncated(e.to_string())
    }
}

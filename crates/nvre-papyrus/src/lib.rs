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
//! # Status, measured
//!
//! **The container is done.** Read and write, both dialects:
//!
//! ```text
//! census     14026/14026   Skyrim Special Edition, Skyrim - Misc.bsa (v3.2)
//!            10015/10015   Skyrim, Skyrim - Misc.bsa (v3.1)
//!             7875/7875    Fallout 4, Fallout4 - Misc.ba2 (v3.9, big structs,
//!                          little-endian)
//! ```
//!
//! Every one parses, writes back byte for byte, and agrees with the game on its
//! own `size` field. That is `examples/census`.
//!
//! **The parser is done.** `examples/parse_census`: **14,301 / 14,301** `.psc`
//! in the CK's `Scripts.zip` parse — every source Bethesda shipped.
//!
//! **The emitter is started, and is not at parity.** It produces a *sound*
//! `.pex` — it writes, re-reads, keeps its size fields right and lands every
//! jump on a real instruction — for real scripts including `Actor.psc`
//! (`examples/emit_probe`). It does **not** yet reproduce the Creation Kit's
//! bytes: `examples/parity` reports 0 identical of 14,026, and the first
//! difference in every one is the string table, because the CK's table is
//! ordered by its own intern order rather than first use.
//!
//! What remains for parity, in the order it is worth doing:
//!
//! 1. **The string table's order.** Every other field depends on getting it
//!    right, because every index in the file is an index into it.
//!    `tools/interning.py` measures how far a candidate order gets. The prefix
//!    is settled and matches every script tried: the script's name, then the
//!    empty string, then `GetState` and `GotoState` (the compiler's own
//!    state-machine pair, present whether or not the source spells them), then
//!    the script's own function names in file order, then its user-flag names,
//!    then the parent class. Beyond that point the order comes from the
//!    per-function body walk and the property records, and is not yet
//!    decoded — which is why the prefix matches and the whole table does not.
//! 2. **The temporary and local ordering** inside a function — the CK's
//!    `::tempN` series is assigned by its own expression walker.
//! 3. **The debug block**, which records a source line per instruction.
//! 4. **Array assignment** (`a[i] = v` through `array_setelement`) and
//!    negative property initial values, both of which the emitter refuses
//!    today rather than guessing.

pub mod emit;
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

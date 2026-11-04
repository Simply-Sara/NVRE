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
//! **The emitter is close, and measured four ways.** Read them in this order,
//! because each one answers a different question and the loosest is the one
//! that reflects whether the code is *right*:
//!
//! ```text
//! examples/behaviour   (54,620 functions, Skyrim SE)
//!     byte-identical                  47,640   87.2%
//!     identical up to temp NUMBERING   4,299    7.9%
//!     behaviourally different          2,681    4.9%
//!   scripts with a behavioural difference   1,546 / 14,026   11.0%
//!
//! examples/structural  48,232 / 54,528 structurally identical after
//!                      renumbering both sides (88.5%)
//! examples/typecensus  51,912 / 54,528 locals type multisets identical (95.2%)
//! examples/parse_census 14,301 / 14,301 .psc parse
//! ```
//!
//! `behaviour` is the headline: it canonicalises each operand by **where its
//! value came from**, so `::temp3` and `::temp7` around the same instructions
//! are the same program, and only a different opcode, operand, branch target or
//! length counts as different.
//!
//! `structural` is the work list `behaviour` cannot give. It renumbers both
//! sides relative to their own temporary base and reports the first instruction
//! that STILL differs, so a uniform offset does not drown the real defects.
//!
//! `typecensus` compares a function's locals table **by multiset of type names**
//! — pure type information, no instructions — which isolates the type model from
//! instruction selection and from the allocator. It is what found that the
//! signature harvest was producing garbage.
//!
//! `examples/parity` compares raw bytes and is *not* a useful headline: the two
//! compilers order their string tables differently, so every index differs even
//! where the code does not. File-level byte identity is also unreachable by
//! nature — the header stamps `compilation_time`, `user_name` and
//! `machine_name`.
//!
//! # Rules the corpus settled
//!
//! Each of these was measured, and each contradicts something plausible:
//!
//! - **The compiler injects `GetState` and `GotoState`** into every script.
//!   Their bodies are fixed. (See below.)
//! - **Every `.pex` carries the user flags `conditional` and `hidden`**, on all
//!   14,026 — including the 2,859 whose source declares neither.
//! - **A function's locals table holds temporaries only.** No `self`, no
//!   parameters, no `::State`. `GetState`'s table is empty.
//! - **An empty function body gets no instructions at all** — not even an
//!   implicit `return None`, and no `::NoneVar`.
//! - **An `auto` property is referenced by its `::Name_var` backing member**,
//!   and a property is an *instance* of its type, never the class: `ExileScene`
//!   is a `Scene` instance, so `ExileScene.IsPlaying()` is a `callmethod`.
//! - **`arr[i] = v` is `array_setelement arr i v`**, not an `assign`, and the
//!   stored value is materialised into a temporary first.
//! - **Primitive type names are always capitalised**: `Int`, `Float`, `Bool`,
//!   `String`, `None`. Only class names vary in case.
//! - **`0x13` is `cmp_gte`**, proven by `achievementsscript` compiling
//!   `statValue >= …` to it. `jmp` is `0x14`.
//!
//! What remains:
//!
//! 1. **The property and function record order.** The Creation Kit stores both
//!    in hash containers and emits them in iteration order — in
//!    `achievementsscript` the injected `GetState` and `GotoState` sit at
//!    positions 5 and 7 of 8, between the script's own. That order is not
//!    source order (29.9%), alphabetical, reverse, type-then-name, length, or
//!    FNV-1a at any bucket count. **This is the one blocker to byte parity.**
//! 2. **The temporary allocator.** The CK reuses `::tempN` where this emitter
//!    allocates a fresh one per expression, so its bodies list ~7 locals where
//!    this one lists ~11.
//! 3. **The debug line table**, one source line per instruction.
//! 4. **Return types for methods the corpus never showed**, which stay `var`.
//!
//! # The string table, decoded
//!
//! The table is the sequence of distinct strings the finished file *mentions*,
//! read in file order. That is measured, not assumed: `tools/interning2.py`
//! tests it against the corpus and it holds for **14,020 of the 14,026**.
//!
//! It is a consequence of something the format docs never say. **The compiler
//! injects `GetState` and `GotoState` into every script.** They appear in no
//! source and in every `.pex`, which is why those two names sit at indices 2
//! and 3 whatever the script is. Their bodies are fixed:
//!
//! ```text
//! String GetState()
//!     {Function that returns the current state}
//!     return ::State
//!
//! None GotoState(String newState)
//!     {Function that switches this object to the specified state}
//!     callmethod onEndState self ::NoneVar  <0>
//!     assign ::State newState
//!     callmethod onBeginState self ::NoneVar <0>
//! ```
//!
//! With them emitted, the first eight indices of `DLC2ThirskDoorScript` are
//! byte-identical to the game's: the script name, `""`, `GetState`,
//! `GotoState`, the script's own body names, then `conditional` and `hidden`.
//!
//! Two more measured facts, both surprising:
//!
//! - **Every `.pex` carries the user flags `conditional` and `hidden`**, on
//!   all 14,026 — including the 2,859 whose source declares neither. Only the
//!   order varies (`conditional` first on 8,730, `hidden` first on 5,296) and
//!   it does not follow the source line. That order is not decoded.
//! - **Property name indices ascend in 3,258 of 3,262 scripts**, so properties
//!   are interned in their file-record order. What decides that record order
//!   is *not* source order (29.9% agreement), alphabetical, reverse,
//!   type-then-name, length, or FNV-1a hash — every one scores at chance.
//!
//! What remains for parity:
//!
//! 1. **The property record order** — the one unknown that still blocks the
//!    middle of the table.
//! 2. **The user-flag order**, `conditional` before `hidden` or the reverse.
//! 3. **The temporary and local ordering** inside a function — the CK's
//!    `::tempN` series is assigned by its own expression walker, and it makes
//!    fewer of them than this emitter does.
//! 4. **The debug block**, which records a source line per instruction.
//!

pub mod check;
pub mod emit;
pub mod globals;
pub mod lex;
pub mod opcodes;
pub mod parse;
pub mod pex;
pub mod returns;
pub mod signatures;
pub mod symbols;

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
    /// Source text could not be parsed, with the position it failed at.
    ///
    /// The position is separate from the message because an editor needs it
    /// structurally — to place a squiggle, to sort diagnostics, to jump to the
    /// line. Formatting it into the message loses that, and it is how a parse
    /// error ended up reported at line 1 for a problem on line 6.
    #[error("{message} (line {line}, column {col})")]
    ParseAt {
        /// The 1-based source line.
        line: u16,
        /// The 1-based source column.
        col: u16,
        /// What went wrong, naming what was expected and what was found.
        message: String,
    },
}

impl From<nvre_util::OutOfBounds> for Error {
    fn from(e: nvre_util::OutOfBounds) -> Self {
        Error::Truncated(e.to_string())
    }
}

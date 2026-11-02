//! The compiler: source text to compiled bytecode.
//!
//! The single entry point. `compile` takes a script's source — the text a
//! `SCPT` record's `SCTX` subrecord holds — and returns the bytes its `SCDA`
//! subrecord holds, which is `[opcode:u16][argLen:u16][args…]` per
//! instruction, little-endian.
//!
//! What it does **not** return is the `SCHR` header's counts. The reference
//! count, the compiled size and the variable count are the record's own, and
//! the game recomputes them when it loads a script — so the compiler returns
//! the body and lets the writer fill the header, rather than guessing at
//! numbers the game owns.
//!
//! # The shape of the work
//!
//! 1. [`crate::lex`] the source into tokens.
//! 2. Group the tokens into lines, and each line into a statement: a
//!    declaration, a `Begin`/`End` block, a control-flow statement, or a
//!    command call.
//! 3. Resolve names: a local variable's index, a reference's index in the
//!    record's `SCRO` list, a command's opcode.
//! 4. Emit each statement, with the expressions inside it in postfix.
//!
//! The acceptance test for all of it is `examples/parity.rs`: compile the
//! source of every unit in `FalloutNV.esm` and compare with the bytes the
//! game wrote. That number is the only honest measure of whether this is
//! right.

/// Compile `source` into a `SCDA` body.
///
/// # Errors
///
/// When the source holds something this compiler does not understand. The
/// message names the line, because a script that will not compile is one the
/// user has to fix, and "line 14" is where they look.
pub fn compile(source: &str) -> Result<Vec<u8>, crate::Error> {
    let script = crate::parse::parse(source);

    // An unreadable line is a compile error rather than something to skip: a
    // body shorter than the source asked for would be a silent bug, and the
    // game would run a script missing a statement the user wrote.
    for stmt in &script.stmts {
        if let crate::parse::Stmt::Unknown { line, text } = stmt {
            return Err(crate::Error::Parse(format!(
                "line {}: cannot read `{text}`",
                line + 1
            )));
        }
    }

    crate::emit::emit(&script)
}

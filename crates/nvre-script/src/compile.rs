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
    compile_with_refs(source, &[], &[], &[])
}

/// Compile `source` with the record's reference entries (from SCRO / plugin
/// context, each with what it names), its variable table (SLSD IDs by name,
/// from SCVR/SLSD), and the variable tables of scripts this record's
/// bytecode names remotely.
///
/// The variable table is what makes the bytecode's variable indices correct:
/// the index is the record's own `SLSD` ID for the name, which no amount of
/// source reading can derive — the GECK assigns IDs with gaps. The remote
/// tables are what make `SomeScript.var` resolve: the var's ID lives in the
/// target script's own record. The reference kinds are what make a standalone
/// operand write the right tag: a GLOB entry compiles as `47` 'G', a placed
/// reference as `5A` 'Z'.
pub fn compile_with_refs(
    source: &str,
    refs: &[crate::parse::RefEntry],
    vars: &[crate::parse::VarEntry],
    remotes: &[crate::parse::RemoteScript],
) -> Result<Vec<u8>, crate::Error> {
    let mut script = crate::parse::parse(source);
    script.refs = refs.to_vec();
    script.vars = vars.to_vec();
    script.remotes = remotes.to_vec();

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

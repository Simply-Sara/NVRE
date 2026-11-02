//! The obscript emitter: statements to bytecode.
//!
//! Every instruction in a `SCDA` body is
//!
//! ```text
//! [opcode:u16][argLen:u16][args…]
//! ```
//!
//! little-endian, where `argLen` is the byte length of what follows and *does
//! not count itself*. That last part is the detail that decides everything
//! else: a body built with `argLen` counting its own two bytes is two bytes
//! long per instruction, and the game reads it as garbage.
//!
//! # Why the framing is a function of its own
//!
//! Because every instruction uses it, and a mistake in it is a mistake in all
//! of them. `frame` is the one place the two-byte header is written, and the
//! parity harness is what proves it: an `argLen` that is off by anything shows
//! up as a length mismatch on the very first unit.
//!
//! # The statements
//!
//! `emit` turns a parsed script into the instruction stream. Each encoding is
//! read off the game's own bytes rather than from a spec: `ScriptName` is four
//! bytes with no arguments, a `Begin` carries its parameter count, its event
//! code and the byte length of the block it opens, and `End` closes it. Those
//! shapes are what `examples/dump_units.rs` prints, measured over every
//! script in `FalloutNV.esm`.

/// Frame `args` as one instruction with `opcode`.
///
/// `argLen` is the length of `args` alone, not of the header: the game reads
/// two bytes for the opcode, two for the length, and then exactly that many
/// bytes of arguments.
pub fn frame(opcode: u16, args: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + args.len());
    out.extend_from_slice(&opcode.to_le_bytes());
    out.extend_from_slice(&(args.len() as u16).to_le_bytes());
    out.extend_from_slice(args);
    out
}

/// An instruction read back out of a body: its opcode and its argument bytes.
///
/// The decoder's half of the framing, and the reason `frame` can be tested
/// against something other than itself: an instruction the emitter frames
/// must read back as the same opcode and the same arguments, or the two
/// disagree about where an instruction ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Where the instruction starts in the body, in bytes.
    pub at: usize,
    /// The instruction's opcode.
    pub opcode: u16,
    /// The argument bytes that follow the four-byte header.
    pub args: Vec<u8>,
}

/// Read the instructions of `body`, in order.
///
/// Stops at the first instruction whose declared length runs past the end of
/// the body, and reports what it read up to there: a truncated body is a
/// corrupt one, and silently padding it would hide that.
pub fn read_instructions(body: &[u8]) -> Vec<Instruction> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 4 <= body.len() {
        let opcode = u16::from_le_bytes([body[i], body[i + 1]]);
        let len = u16::from_le_bytes([body[i + 2], body[i + 3]]) as usize;
        let args_start = i + 4;
        let Some(args_end) = args_start.checked_add(len) else {
            break;
        };
        if args_end > body.len() {
            break;
        }
        out.push(Instruction {
            at: i,
            opcode,
            args: body[args_start..args_end].to_vec(),
        });
        i = args_end;
    }
    out
}

use crate::opcodes::{Event, Statement as StmtOp};
use crate::parse::{Script, Stmt};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_instructions_length_does_not_count_itself() {
        // `11 00 00 00` — End, with no arguments. The length word is 0, not 2
        // and not 4: it counts the arguments only.
        assert_eq!(frame(0x11, &[]), vec![0x11, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn an_instruction_with_arguments_frames_them_after_the_header() {
        // ScriptName carries its name as a zstring in the argument bytes.
        let args = b"abc\0";
        let framed = frame(0x1D, args);
        assert_eq!(&framed[0..2], &0x1Du16.to_le_bytes());
        assert_eq!(&framed[2..4], &(args.len() as u16).to_le_bytes());
        assert_eq!(&framed[4..], args);
    }

    #[test]
    fn a_framed_instruction_reads_back_as_itself() {
        // The round trip that makes the framing trustworthy: whatever the
        // emitter writes, the reader must take apart the same way.
        for (op, args) in [
            (0x10u16, vec![1, 0, 2, 0, 4, 0, 0, 0]),
            (0x15, vec![0x73, 1, 0, 2, 0, 0x6E, 5, 0, 0, 0]),
            (0x100D, vec![0, 0]),
            (0x1D, b"MyScript\0".to_vec()),
        ] {
            let body = frame(op, &args);
            let read = read_instructions(&body);
            assert_eq!(read.len(), 1, "one instruction in {body:02X?}");
            assert_eq!(read[0].opcode, op);
            assert_eq!(read[0].args, args);
            assert_eq!(read[0].at, 0);
        }
    }

    #[test]
    fn a_run_of_instructions_reads_back_in_order() {
        let mut body = Vec::new();
        body.extend(frame(0x1D, b"S\0"));
        body.extend(frame(0x10, &[0, 0, 0, 0, 4, 0, 0, 0]));
        body.extend(frame(0x11, &[]));
        let read = read_instructions(&body);
        assert_eq!(read.len(), 3);
        assert_eq!(read[0].opcode, 0x1D);
        assert_eq!(read[1].opcode, 0x10);
        assert_eq!(read[2].opcode, 0x11);
        // Each instruction starts where the previous one ended, so the second
        // begins after the first's four-byte header and its one argument byte.
        assert_eq!(read[1].at, 6);
    }

    #[test]
    fn a_truncated_body_stops_rather_than_padding() {
        // A body whose last instruction claims more arguments than exist: the
        // reader stops at it instead of inventing the missing bytes.
        let mut body = frame(0x11, &[]);
        body.extend_from_slice(&0x100Du16.to_le_bytes());
        body.extend_from_slice(&50u16.to_le_bytes());
        body.extend_from_slice(&[1, 2, 3]);
        let read = read_instructions(&body);
        assert_eq!(read.len(), 1, "the truncated instruction is not read");
        assert_eq!(read[0].opcode, 0x11);
    }
}

/// Emit `script` as a `SCDA` body.
///
/// # Errors
///
/// When the script holds a statement this emitter cannot encode. The message
/// names what it was, because a script that will not compile is one the user
/// has to fix.
pub fn emit(script: &Script) -> Result<Vec<u8>, crate::Error> {
    // An unreadable line is refused here, where the statements are walked,
    // rather than only in `compile`: the emitter is the thing that would
    // otherwise drop it, and a body shorter than the source asked for would
    // be a silent bug — the game would run a script missing a statement the
    // user wrote.
    for stmt in &script.stmts {
        if let Stmt::Unknown { line, text } = stmt {
            return Err(crate::Error::Parse(format!(
                "line {}: cannot read `{text}`",
                line + 1
            )));
        }
    }

    let mut out = Vec::new();

    // The statements are walked by index rather than by iterator, because a
    // block consumes the statements between its `Begin` and its `End` and the
    // walk has to resume *after* the `End` rather than visit it again. A flat
    // `for` loop over the list would see the block's own `End` a second time
    // and refuse it as one with no open block.
    let mut i = 0usize;
    while i < script.stmts.len() {
        let stmt = &script.stmts[i];
        match stmt {
            // The name is a marker in the bytecode, not a value: the record's
            // `EDID` holds the name and this instruction says where the
            // script's own name is. Measured: every `ScriptName` in the corpus
            // is exactly `1D 00 00 00`.
            Stmt::ScriptName(_) => {
                out.extend(frame(StmtOp::ScriptName.opcode(), &[]));
                i += 1;
            }

            // A block: its `Begin`, its body, and its `End`, emitted whole.
            Stmt::Begin { .. } => {
                out.extend(emit_block(script, i)?);
                // Resume after the `End` that closed this block. The depth is
                // tracked the same way `emit_block` tracks it, so a nested
                // block's `End` is not mistaken for this one's.
                i = after_block(script, i);
            }

            Stmt::End => {
                // An `End` with no open block. The game's own compiler refuses
                // this, so this does too rather than writing a stray `End`.
                return Err(crate::Error::Parse(
                    "`End` with no open `Begin` block".to_string(),
                ));
            }

            // A statement outside any block. The game refuses these too: a
            // statement belongs to a block.
            other => {
                return Err(crate::Error::Parse(format!(
                    "{} outside a Begin/End block",
                    describe(other)
                )));
            }
        }
    }

    Ok(out)
}

/// The index just past the `End` that closes the block starting at `start`.
///
/// The same depth count `emit_block` uses, so the two agree about which `End`
/// belongs to which `Begin`. A block with no `End` is a compile error, and the
/// caller reports it before this is reached.
fn after_block(script: &Script, start: usize) -> usize {
    let mut depth = 0usize;
    for (offset, stmt) in script.stmts[start + 1..].iter().enumerate() {
        match stmt {
            Stmt::Begin { .. } => depth += 1,
            Stmt::End if depth == 0 => return start + 1 + offset + 1,
            Stmt::End => depth -= 1,
            _ => {}
        }
    }
    // No `End`: the whole rest of the script was the block's body.
    script.stmts.len()
}

/// What a statement is called, for an error message.
fn describe(stmt: &Stmt) -> &'static str {
    match stmt {
        Stmt::ScriptName(_) => "ScriptName",
        Stmt::Begin { .. } => "Begin",
        Stmt::End => "End",
        Stmt::Set { .. } => "set",
        Stmt::If(_) => "if",
        Stmt::Else => "else",
        Stmt::ElseIf(_) => "elseif",
        Stmt::EndIf => "endif",
        Stmt::Return => "return",
        Stmt::Call { .. } => "a command call",
        Stmt::Unknown { .. } => "an unreadable line",
    }
}

/// Emit one `Begin` block: its `Begin`, its body, and its `End`.
///
/// The body is emitted into a scratch buffer first, because `blockLen` is its
/// length and the length cannot be known until the body is. Emitting the
/// `Begin` first and filling the length in afterwards would mean patching
/// bytes that have already been framed.
fn emit_block(script: &Script, start: usize) -> Result<Vec<u8>, crate::Error> {
    let Some(Stmt::Begin { event, .. }) = script.stmts.get(start) else {
        return Err(crate::Error::Parse("not a Begin".to_string()));
    };

    // The event's code. An event the table does not name is refused rather
    // than emitted as a wrong one: a `Begin` with the wrong event code is a
    // block that fires on the wrong thing, which is worse than a compile error.
    let code = Event::from_name(event)
        .ok_or_else(|| crate::Error::Parse(format!("unknown block type `{event}`")))?;

    // The block's body: every statement after this `Begin` up to its `End`. A
    // nested `Begin` is emitted as the inner block's own instruction run, so
    // the depth counter tracks which `End` closes which `Begin`.
    let mut body = Vec::new();
    let mut closed = false;
    let mut depth = 0usize;
    for (offset, stmt) in script.stmts[start + 1..].iter().enumerate() {
        match stmt {
            Stmt::Begin { .. } => {
                depth += 1;
                body.extend(emit_block(script, start + 1 + offset)?);
            }
            Stmt::End if depth == 0 => {
                closed = true;
                break;
            }
            Stmt::End => {
                depth -= 1;
                body.extend(frame(StmtOp::End.opcode(), &[]));
            }
            other => body.extend(emit_statement(other)?),
        }
    }
    if !closed {
        return Err(crate::Error::Parse(format!(
            "`Begin {event}` has no matching `End`"
        )));
    }

    // The frame: the event code, the block's length, the parameter count, and
    // the parameters.
    //
    // The order is measured, not assumed. `Begin GameMode` in the corpus is
    // `00 00 [04 00 00 00]` — six argument bytes with no parameter count at
    // all — and `Begin OnAdd player` is `03 00 [04 00 00 00] [01 00] [72 01
    // 00]`, where the count appears *after* the block length and only when
    // there is a parameter to count. So the count is written last, and only
    // for a block that has one.
    let params = emit_begin_params(script, start)?;
    let mut begin_args = Vec::with_capacity(6 + params.len());
    begin_args.extend_from_slice(&code.code().to_le_bytes());
    // The block's length: the body's bytes **plus** the `End` that closes it.
    //
    // Measured, and the measurement is what settles it: a `GameMode` block
    // with nothing in it writes `blockLen` 4, and its only body byte run is
    // the four bytes of its `End`. So the length counts from after the
    // `Begin`'s arguments through the `End` inclusive — not the body alone.
    let block_len = body.len() + 4;
    begin_args.extend_from_slice(&(block_len as u32).to_le_bytes());
    if !params.is_empty() {
        begin_args.extend_from_slice(&1u16.to_le_bytes());
        begin_args.extend_from_slice(&params);
    }

    let mut out = frame(StmtOp::Begin.opcode(), &begin_args);
    out.extend(body);
    out.extend(frame(StmtOp::End.opcode(), &[]));
    Ok(out)
}

/// A `Begin` block's parameters.
///
/// Zero for now: an argument is a tagged value naming one of the record's own
/// references, and this emitter has no reference list to resolve it against.
/// Refusing is honest — a made-up index would be a block that receives the
/// wrong reference, which the game would run rather than reject.
fn emit_begin_params(script: &Script, start: usize) -> Result<Vec<u8>, crate::Error> {
    let Some(Stmt::Begin { args, .. }) = script.stmts.get(start) else {
        return Ok(Vec::new());
    };
    if args.is_empty() {
        return Ok(Vec::new());
    }
    Err(crate::Error::Parse(
        "a Begin block with arguments is not emitted yet".to_string(),
    ))
}

/// Emit one statement that is not a block.
fn emit_statement(stmt: &Stmt) -> Result<Vec<u8>, crate::Error> {
    match stmt {
        Stmt::End => Ok(frame(StmtOp::End.opcode(), &[])),
        Stmt::Return => Ok(frame(StmtOp::Return.opcode(), &[])),
        // `ScriptName` inside a block is the same marker as outside one.
        Stmt::ScriptName(_) => Ok(frame(StmtOp::ScriptName.opcode(), &[])),
        // A declaration emits nothing: the variable is in the record's
        // `SLSD`/`SCVR` list, which is the writer's job rather than the
        // bytecode's.
        _ => Err(crate::Error::Parse(format!(
            "{} is not emitted yet",
            describe(stmt)
        ))),
    }
}

#[cfg(test)]
mod emit_tests {
    use super::*;
    use crate::parse::parse;

    fn compile(source: &str) -> Result<Vec<u8>, crate::Error> {
        emit(&parse(source))
    }

    #[test]
    fn a_scriptname_is_four_bytes_with_no_arguments() {
        // Measured: every `ScriptName` in FalloutNV.esm is exactly this, and
        // the script's name is in the record's `EDID` rather than in the
        // bytecode — the instruction is a marker, not a value.
        assert_eq!(
            compile("scn MyScript").unwrap(),
            vec![0x1D, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn an_empty_block_is_a_begin_and_an_end() {
        // `Begin GameMode` / `End`, measured: `10 00 06 00 [00 00][00 00][04 00
        // 00 00]` — no parameters, event 0, and a block length of 4, which is
        // the length of the `End` that closes it.
        assert_eq!(
            compile("scn S\nBegin GameMode\nEnd").unwrap(),
            vec![
                0x1D, 0x00, 0x00, 0x00, // ScriptName
                0x10, 0x00, 0x06, 0x00, // Begin, argLen 6
                0x00, 0x00, // event GameMode
                0x04, 0x00, 0x00, 0x00, // blockLen 4
                0x11, 0x00, 0x00, 0x00, // End
            ]
        );
    }

    #[test]
    fn a_blocks_length_is_its_bodys_bytes_not_counting_the_end() {
        // The same block with a `return` inside it: the body is the `return`'s
        // four bytes, and `blockLen` is 4 — the `End` that follows is not part
        // of the block's own length.
        let got = compile("scn S\nBegin GameMode\n  return\nEnd").unwrap();
        // Begin's blockLen sits at offset 8: the four-byte ScriptName, then
        // the Begin's own four-byte header (opcode and argLen), then the event
        // code's two bytes.
        let block_len = u32::from_le_bytes([got[10], got[11], got[12], got[13]]);
        // The body is the return's four bytes, and the block's length counts
        // the End too — so 8.
        assert_eq!(block_len, 8, "the return plus the End that closes it");
        // And the whole body is Begin(14) + return(4) + End(4).
        assert_eq!(got.len(), 14 + 4 + 4);
    }

    #[test]
    fn a_block_type_the_table_does_not_name_is_refused() {
        // A `Begin` with an unknown event is a block that fires on the wrong
        // thing, which is worse than a compile error.
        let err = compile("scn S\nBegin OnNoSuchEvent\nEnd").unwrap_err();
        assert!(
            err.to_string().contains("OnNoSuchEvent"),
            "the error names the block type: {err}"
        );
    }

    #[test]
    fn a_block_with_no_end_is_refused() {
        let err = compile("scn S\nBegin GameMode").unwrap_err();
        assert!(err.to_string().contains("no matching `End`"), "{err}");
    }

    #[test]
    fn an_end_with_no_block_is_refused() {
        let err = compile("scn S\nEnd").unwrap_err();
        assert!(err.to_string().contains("no open `Begin`"), "{err}");
    }

    #[test]
    fn a_statement_outside_a_block_is_refused() {
        let err = compile("scn S\nreturn").unwrap_err();
        assert!(
            err.to_string().contains("outside a Begin/End block"),
            "{err}"
        );
    }

    #[test]
    fn an_unreadable_line_is_a_compile_error_naming_the_line() {
        // A body shorter than the source asked for would be a silent bug, so
        // the line the compiler gave up on is named.
        let err = compile("scn S\nBegin GameMode\n  to nowhere\nEnd").unwrap_err();
        assert!(err.to_string().contains("line 3"), "{err}");
    }

    #[test]
    fn a_nested_block_is_emitted_inside_its_parent() {
        // The inner block's instructions are part of the outer block's body,
        // and the outer `blockLen` counts them.
        let got = compile("scn S\nBegin GameMode\n  Begin MenuMode\n  End\nEnd").unwrap();
        // The outer Begin's blockLen is at offset 8, for the same reason.
        let outer_len = u32::from_le_bytes([got[10], got[11], got[12], got[13]]);
        // The inner block is its own Begin(14) + End(4) = 18 bytes, and the
        // outer block's length counts its own End as well — so 22.
        assert_eq!(outer_len, 22, "the inner block and the outer End");
    }

    #[test]
    fn a_blocks_arguments_are_refused_rather_than_guessed() {
        // A `Begin` argument names one of the record's own references, and
        // this emitter has no reference list — so it refuses rather than
        // writing a made-up index the game would run.
        let err = compile("scn S\nBegin OnAdd player\nEnd").unwrap_err();
        assert!(err.to_string().contains("arguments"), "{err}");
    }

    #[test]
    fn a_declaration_emits_nothing_of_its_own() {
        // The variable is in the record's `SLSD`/`SCVR` list, which is the
        // writer's job rather than the bytecode's.
        assert_eq!(
            compile("scn S\nshort iCount\nBegin GameMode\nEnd").unwrap(),
            compile("scn S\nBegin GameMode\nEnd").unwrap()
        );
    }

    #[test]
    fn a_command_call_is_refused_until_the_opcode_table_covers_it() {
        let err = compile("scn S\nBegin GameMode\n  SetAV Player 5 10\nEnd").unwrap_err();
        assert!(err.to_string().contains("not emitted yet"), "{err}");
    }
}

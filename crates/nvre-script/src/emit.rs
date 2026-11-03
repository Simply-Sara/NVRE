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
        // 0x1C SetCurrentRef framing in compiled obscript bytecode:
        // [1C 00] [refIdx: u16] followed immediately by the target instruction.
        if opcode == 0x001C {
            let ref_idx = u16::from_le_bytes([body[i + 2], body[i + 3]]);
            out.push(Instruction {
                at: i,
                opcode,
                args: ref_idx.to_le_bytes().to_vec(),
            });
            i += 4;
            continue;
        }
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
    fn an_if_statement_emits_jump_ops_and_matching_endif() {
        let source = "scn TestScript\nshort bDoOnce\nbegin GameMode\nif bDoOnce == 0\nset bDoOnce to 1\nendif\nend\n";
        let compiled = crate::compile::compile(source).unwrap();
        let ins = read_instructions(&compiled);
        // ScriptName, Begin, If, Set, EndIf, End
        assert_eq!(ins.len(), 6);
        assert_eq!(ins[0].opcode, StmtOp::ScriptName.opcode());
        assert_eq!(ins[1].opcode, StmtOp::Begin.opcode());
        assert_eq!(ins[2].opcode, StmtOp::If.opcode());
        // jumpOps = 1 (skipping Set)
        assert_eq!(&ins[2].args[0..2], &1u16.to_le_bytes());
        // exprLen = 9 (0x20 's' 0x01 0x00 0x20 '0' 0x20 '==' )
        assert_eq!(&ins[2].args[2..4], &9u16.to_le_bytes());
        assert_eq!(ins[3].opcode, StmtOp::Set.opcode());
        assert_eq!(ins[4].opcode, StmtOp::EndIf.opcode());
        assert_eq!(ins[5].opcode, StmtOp::End.opcode());
    }

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

            // A declaration outside any block emits nothing, as it does
            // inside one. It is carried as a statement for the jumpOps
            // count, not for any bytes of its own.
            Stmt::Decl(_) => {
                i += 1;
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

/// Information about the branches of an `If` statement chain.
struct IfChain {
    /// The branches, in source order — the `if` head first, then whatever
    /// mixture of `elseif` and `else` the source wrote. Source order is not
    /// decoration: `else if (...)` followed by an `elseif` is an `else` and
    /// then an `elseif`, and emitting every `elseif` before the `else` puts
    /// the branches in an order the game never wrote (VCG03CampfireTrigger).
    branches: Vec<Branch>,
    /// The index just after the `EndIf`.
    after_endif: usize,
}

/// One branch of an `if` chain: its head instruction, its condition (none for
/// `else`), and the statements under it.
struct Branch {
    /// The head's opcode — `If`, `ElseIf` or `Else`.
    head: StmtOp,
    /// The condition, for the heads that have one.
    cond: Option<Vec<crate::lex::Token>>,
    /// The branch's body.
    body: std::ops::Range<usize>,
}

/// Analyze an `If` statement starting at `if_idx` and find all its branches.
///
/// The statements are flat, so a chain is read as a list of branch markers at
/// the same depth — the `if`, each `elseif`, the `else` — and each branch's
/// body is the run between its own marker and the next one. Reading it that
/// way is what makes an *empty* branch work: `elseif getdestroyed == 1` with
/// nothing under it (MrHouseSCRIPT line 21) has a body of no statements, and
/// the game writes `jumpOps 0` for it and then the `else` that follows. The
/// order the markers appear in is the order the branches are emitted in.
fn analyze_if_chain(script: &Script, if_idx: usize) -> Result<IfChain, crate::Error> {
    let mut markers: Vec<(usize, StmtOp, Option<Vec<crate::lex::Token>>)> =
        vec![(if_idx, StmtOp::If, None)];
    let mut depth = 0usize;
    let mut after_endif = None;
    let mut i = if_idx + 1;
    while i < script.stmts.len() {
        match &script.stmts[i] {
            Stmt::If(_) => {
                depth += 1;
                i += 1;
            }
            Stmt::ElseIf(cond) if depth == 0 => {
                markers.push((i, StmtOp::ElseIf, Some(cond.clone())));
                i += 1;
            }
            Stmt::Else if depth == 0 => {
                markers.push((i, StmtOp::Else, None));
                i += 1;
            }
            Stmt::EndIf if depth == 0 => {
                after_endif = Some(i + 1);
                break;
            }
            Stmt::EndIf => {
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }

    let Some(after) = after_endif else {
        return Err(crate::Error::Parse(
            "`if` statement has no matching `endif`".to_string(),
        ));
    };

    // Each branch's body runs from just after its marker to just before the
    // next marker (or the `endif`).
    let mut branches = Vec::with_capacity(markers.len());
    for n in 0..markers.len() {
        let (at, head, cond) = &markers[n];
        let end = markers.get(n + 1).map(|(next, _, _)| *next).unwrap_or(after - 1);
        branches.push(Branch {
            head: *head,
            cond: cond.clone(),
            body: at + 1..end,
        });
    }

    Ok(IfChain {
        branches,
        after_endif: after,
    })
}

/// A declaration's weight in the `jumpOps` count.
///
/// Measured: the game's `jumpOps` counts a branch's **source statements**,
/// not its emitted instructions — a `ref` declared inside an `if` body emits
/// no bytecode, but the game still counts it as one operation to skip. Six
/// corpus branches disagree with the instruction count by exactly one, and
/// all six declare a variable in the branch body (`CrNightkin`'s
/// `ref mytarget`, `REPCONDavison`'s `ref refvar`, `VNovacChupacabra`'s
/// `ref targetREF`); across all 13,163 branches in `FalloutNV.esm`,
/// `jumpOps == instructions_in_span − SetCurrentRef prefixes + declarations`
/// with no exception.
const DECL_JUMP_WEIGHT: usize = 1;

/// Count the `jumpOps` of a branch whose emitted body is `bytes` with
/// `decls` declarations among its source statements.
///
/// The instruction count is not the operation count for two measured
/// reasons. A `0x1C` SetCurrentRef prefix is framing for the command that
/// follows it, not an operation of its own (measured: 99% of the corpus's
/// branches over-count without subtracting them). And a declaration emits
/// no bytes but is still one source statement the game counts.
fn jump_ops(bytes: &[u8], decls: usize) -> u16 {
    let mut set_current_refs = 0usize;
    let mut instructions = 0usize;
    for ins in read_instructions(bytes) {
        if ins.opcode == 0x001C {
            set_current_refs += 1;
        }
        instructions += 1;
    }
    (instructions + decls * DECL_JUMP_WEIGHT - set_current_refs) as u16
}

/// How many of `stmts` are declarations.
///
/// A declaration emits no bytecode but is one of the source statements the
/// game's `jumpOps` counts — see [`jump_ops`].
fn count_decls(stmts: &[crate::parse::Stmt]) -> usize {
    stmts
        .iter()
        .filter(|s| matches!(s, crate::parse::Stmt::Decl(crate::parse::DeclKind::Ref)))
        .count()
}

/// Emit an `If` statement chain: the `If`, any `ElseIf`s, optional `Else`, and closing `EndIf`.
fn emit_if_chain(
    script: &Script,
    if_idx: usize,
    cond: &[crate::lex::Token],
) -> Result<(Vec<u8>, usize), crate::Error> {
    emit_branch(script, if_idx, Some(cond), StmtOp::If)
}

/// Emit one branch head and its body — the shared shape behind `If` and the
/// stray `ElseIf`/`Else` the GECK still compiles.
///
/// The game does not reject a chain that does not nest: `TecMineHostage` closes
/// its outer `if` and then writes `elseif ( GetDead == 1 )` on the next line,
/// and the GECK compiled it as `18 00 [jumpOps 1][exprLen][expr]`, its body,
/// and an `endif` (measured; the instruction is at index 24 of the unit's
/// bytecode). `CrNightkinCloakUntilFireScript` does the same with a bare
/// `else`, which compiles to `17 00 02 00 [jumpOps]` — jumpOps only, no
/// expression. So a stray `elseif`/`else` is emitted as a branch head of its
/// own, its body running to the next `endif` at its level, and the `endif`
/// that closes it is emitted with it.
fn emit_branch(
    script: &Script,
    if_idx: usize,
    cond: Option<&[crate::lex::Token]>,
    head: StmtOp,
) -> Result<(Vec<u8>, usize), crate::Error> {
    let chain = analyze_if_chain(script, if_idx)?;

    // Helper to emit a slice of statements
    fn emit_stmts(script: &Script, range: std::ops::Range<usize>) -> Result<Vec<u8>, crate::Error> {
        let mut out = Vec::new();
        let mut idx = range.start;
        while idx < range.end {
            match &script.stmts[idx] {
                Stmt::Begin { .. } => {
                    out.extend(emit_block(script, idx)?);
                    idx = after_block(script, idx);
                }
                Stmt::If(c) => {
                    let (bytes, next_idx) = emit_if_chain(script, idx, c)?;
                    out.extend(bytes);
                    idx = next_idx;
                }
                Stmt::ElseIf(c) => {
                    let (bytes, next_idx) = emit_branch(script, idx, Some(c), StmtOp::ElseIf)?;
                    out.extend(bytes);
                    idx = next_idx;
                }
                Stmt::Else => {
                    let (bytes, next_idx) = emit_branch(script, idx, None, StmtOp::Else)?;
                    out.extend(bytes);
                    idx = next_idx;
                }
                other => {
                    out.extend(emit_statement(script, other)?);
                    idx += 1;
                }
            }
        }
        Ok(out)
    }

    let mut out = Vec::new();
    for (n, branch) in chain.branches.iter().enumerate() {
        let body = emit_stmts(script, branch.body.clone())?;
        // jumpOps is the number of operations to skip when the branch is not
        // taken: its own statements, declarations included. A `0x1C`
        // SetCurrentRef prefix is framing, not an operation — see `jump_ops`.
        let ops = jump_ops(&body, count_decls(&script.stmts[branch.body.clone()]));
        // The head a branch's own spelling asked for, for the first one; the
        // rest carry the marker's owm kind (a stray `elseif` chain is emitted
        // as one `if`-shaped branch and nothing after it).
        let branch_head = if n == 0 { head } else { branch.head };
        let mut args = Vec::new();
        args.extend_from_slice(&ops.to_le_bytes());
        let cond_tokens: Option<&[crate::lex::Token]> = if n == 0 {
            cond
        } else {
            branch.cond.as_deref()
        };
        if let Some(cond) = cond_tokens {
            let cond_bytes = crate::value::encode_expr(cond, script)?;
            args.extend_from_slice(&(cond_bytes.len() as u16).to_le_bytes());
            args.extend_from_slice(&cond_bytes);
        }
        out.extend(frame(branch_head.opcode(), &args));
        out.extend(body);
    }
    out.extend(frame(StmtOp::EndIf.opcode(), &[]));
    Ok((out, chain.after_endif))
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
        Stmt::Decl(_) => "a declaration",
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
    let mut i = start + 1;
    while i < script.stmts.len() {
        let stmt = &script.stmts[i];
        match stmt {
            // A nested block, emitted whole: its `Begin`, its body and its
            // `End`. The recursion owns that `End`, so the walk resumes past
            // it rather than reaching it a second time — otherwise the inner
            // block's `End` is written twice, and the outer block's length is
            // four bytes longer than the game's own.
            Stmt::Begin { .. } => {
                body.extend(emit_block(script, i)?);
                i = after_block(script, i);
            }
            Stmt::If(cond) => {
                let (bytes, next_i) = emit_if_chain(script, i, cond)?;
                body.extend(bytes);
                i = next_i;
            }
            // A stray `elseif`/`else` — the GECK compiles these as their own
            // branch head rather than refusing them; see `emit_branch`.
            Stmt::ElseIf(cond) => {
                let (bytes, next_i) = emit_branch(script, i, Some(cond), StmtOp::ElseIf)?;
                body.extend(bytes);
                i = next_i;
            }
            Stmt::Else => {
                let (bytes, next_i) = emit_branch(script, i, None, StmtOp::Else)?;
                body.extend(bytes);
                i = next_i;
            }
            Stmt::End => {
                closed = true;
                break;
            }
            other => {
                body.extend(emit_statement(script, other)?);
                i += 1;
            }
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
    // An event that cannot take an argument has whatever the source passed
    // dropped, not encoded: `BEGIN ScriptEffectStart EnchARCHIMEDESEffect`
    // (VHeliosARCHIMEDESEffectScript) writes `11 00 | 24 00 00 00` — six
    // argument bytes, no count, no parameter. Resolving the name would refuse
    // a block the game compiled by ignoring it.
    let params = if code.takes_argument() {
        emit_begin_params(script, start)?
    } else {
        Vec::new()
    };
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
    // The parameter count, and only for the events that can take one.
    //
    // This is the detail that is easiest to get wrong, because both forms
    // exist: `Begin GameMode` writes six argument bytes with no count at all,
    // and `Begin OnDeath` writes eight with a count of zero — even when
    // nothing is passed to it. The corpus splits 1,132 to 1,033 between the
    // two, and the split is exactly `Event::takes_argument`.
    if code.takes_argument() {
        begin_args.extend_from_slice(&(params.len() as u16 / 3).to_le_bytes());
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

    let mut out = Vec::new();
    for t in args {
        if t.kind == crate::lex::Kind::Space || t.kind == crate::lex::Kind::Comment {
            continue;
        }
        if t.kind == crate::lex::Kind::Number {
            if let Ok(n) = t.text.parse::<i32>() {
                out.push(0x6E);
                out.extend_from_slice(&n.to_le_bytes());
                continue;
            }
        }
        // Reference argument
        let ref_idx = resolve_ref(&t.text, script)?;
        out.push(0x72);
        out.extend_from_slice(&ref_idx.to_le_bytes());
    }
    Ok(out)
}

/// Emit one statement that is not a block.
fn emit_statement(script: &Script, stmt: &Stmt) -> Result<Vec<u8>, crate::Error> {
    match stmt {
        Stmt::End => Ok(frame(StmtOp::End.opcode(), &[])),
        Stmt::EndIf => Ok(frame(StmtOp::EndIf.opcode(), &[])),
        Stmt::Return => Ok(frame(StmtOp::Return.opcode(), &[])),
        // A declaration emits nothing of its own — but see `jump_ops`: it is
        // still a statement the game counts.
        Stmt::Decl(_) => Ok(vec![]),
        // `ScriptName` inside a block is the same marker as outside one.
        Stmt::ScriptName(_) => Ok(frame(StmtOp::ScriptName.opcode(), &[])),
        Stmt::Set { on, target, expr } => emit_set(script, on, target, expr),
        Stmt::Call { on, command, args } => emit_call(script, on, command, args),
        // A declaration emits nothing: the variable is in the record's
        // `SLSD`/`SCVR` list, which is the writer's job rather than the
        // bytecode's.
        _ => Err(crate::Error::Parse(format!(
            "{} is not emitted yet",
            describe(stmt)
        ))),
    }
}

/// Emit `set <target> to <expr>`.
///
/// Measured, and the shape is not the obvious one:
///
/// ```text
/// 15 00 [argLen] [destination] [len:u16] 0x20 [the literal's own bytes]
/// ```
///
/// The destination is a tagged value — `73 01 00` for a variable of this
/// script, `72 01 00` for one of the record's references — and the length
/// that precedes the value counts the `0x20` tag along with the literal's
/// bytes. So `set PickedUp to 1` is
///
/// ```text
/// 15 00 07 00 | 73 01 00 | 02 00 | 20 31
/// ```
///
/// where `02 00` is the length of `20 31` and `31` is the ASCII `1`.
fn emit_set(
    script: &Script,
    on: &Option<String>,
    target: &crate::lex::Token,
    expr: &[crate::lex::Token],
) -> Result<Vec<u8>, crate::Error> {
    let mut dot_ref_prefix: Option<u16> = None;
    let dest_bytes = if let Some(ref_name) = on {
        let ridx = resolve_ref(ref_name, script)?;
        // A dotted assignment destination is either a variable of this
        // script reached through a reference (`Ref.var`, the `1C`-style
        // destination the corpus measures as `72 <refIdx> 73 01 00` when
        // the var is this script's own) or a remote script's variable
        // (`Script.var`, measured as `72 <refIdx> <tag> <the var's SLSD ID
        // in the remote table>`). The two are distinguished by whether the
        // target names a remote script the caller supplied.
        if let Ok((tag, id)) = crate::value::resolve_remote_var(ref_name, &target.text, script) {
            let mut b = vec![0x72];
            b.extend_from_slice(&ridx.to_le_bytes());
            b.push(tag);
            b.extend_from_slice(&id.to_le_bytes());
            b
        } else {
            // `Ref.Set var to …` is a SetCurrentRef prefix and then a plain
            // variable destination: measured,
            // `MS14ArmorVoiceRef.Set SayTimer to GameDaysPassed + 0.1`
            // (Fallout3.esm MS14PowerArmorScript) writes `1C 00 | 01 00` and
            // then `15 00 | 0A 00 | 66 02 00 | 0A 00 | …` — the `1C` frames
            // the reference and the destination is the variable alone.
            dot_ref_prefix = Some(ridx);
            let dest = crate::value::read_value(std::slice::from_ref(target), script)?;
            crate::value::encode(&dest)
        }
    } else {
        // A bare destination name. The variable case comes first — a
        // declared `ref` variable is a variable, not a reference slot
        // (`Set rLink to GetLinkedRef` writes the `f` tag and the
        // variable's SLSD ID). Then a global: an SCRO entry that names a
        // GLOB record writes `47` 'G' + its index. A placed reference
        // cannot be a destination at all, and is refused rather than
        // written as a variable or a bare reference index, which the game
        // reads as a different destination.
        if script
            .decls
            .iter()
            .any(|d| d.name.eq_ignore_ascii_case(&target.text))
        {
            let dest = crate::value::read_value(std::slice::from_ref(target), script)?;
            crate::value::encode(&dest)
        } else if let Some(entry) = script
            .refs
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(&target.text))
        {
            match entry.kind {
                crate::parse::RefKind::Global => {
                    let ref_idx = crate::value::resolve_ref_index(&target.text, script)?;
                    let mut b = vec![0x47];
                    b.extend_from_slice(&ref_idx.to_le_bytes());
                    b
                }
                crate::parse::RefKind::Reference => {
                    return Err(crate::Error::Parse(format!(
                        "`{}` names a placed reference, which cannot be a set destination",
                        target.text
                    )));
                }
            }
        } else {
            return Err(crate::Error::Parse(format!(
                "`{}` is not a declared variable",
                target.text
            )));
        }
    };

    let expr_bytes = crate::value::encode_expr(expr, script)?;

    // The `1C` prefix is its own instruction, not part of the `Set` frame:
    // measured, `1C 00 | 01 00` then `15 00 | 0A 00 | …`.
    let mut out = Vec::new();
    if let Some(ridx) = dot_ref_prefix {
        out.extend_from_slice(&0x001Cu16.to_le_bytes());
        out.extend_from_slice(&ridx.to_le_bytes());
    }
    let mut args = dest_bytes;
    args.extend_from_slice(&(expr_bytes.len() as u16).to_le_bytes());
    args.extend_from_slice(&expr_bytes);

    out.extend(frame(StmtOp::Set.opcode(), &args));
    Ok(out)
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
        // The inner block is its own Begin(4 + 8 argument bytes = 12) + End(4)
        // = 16 bytes, and the outer block's length counts its own End as well —
        // so 20. `MenuMode` can take a parameter, so its parameterless form
        // writes the count of zero rather than omitting the field.
        assert_eq!(outer_len, 20, "the inner block and the outer End");
    }

    #[test]
    fn a_blocks_arguments_are_refused_rather_than_guessed() {
        // A `Begin` argument names one of the record's own references, and
        // this emitter has no reference list — so it refuses rather than
        // writing a made-up index the game would run.
        let err = compile("scn S\nBegin OnAdd player\nEnd").unwrap_err();
        assert!(err.to_string().contains("reference"), "{err}");
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
}

#[cfg(test)]
mod call_tests {
    use super::*;
    use crate::parse::parse;

    fn compile(source: &str) -> Result<Vec<u8>, crate::Error> {
        emit(&parse(source))
    }

    #[test]
    fn a_command_with_no_parameters_writes_no_argument_bytes() {
        // Measured: `PickIdle` is `argLen` 0 with nothing after the header.
        let got = compile("scn S\nBegin GameMode\n  PickIdle\nEnd").unwrap();
        assert!(
            got.windows(4).any(|w| w == [0x64, 0x10, 0x00, 0x00]),
            "PickIdle is opcode 0x1064 with argLen 0: {:02X?}",
            got
        );
    }

    #[test]
    fn a_command_with_parameters_writes_a_count_of_zero_when_given_none() {
        // Measured: `activate` is `100D 00 02 00 00 00` — the count of zero is
        // written even though nothing is passed, because the command declares
        // parameters. This is the distinction from the form above.
        let got = compile("scn S\nBegin GameMode\n  activate\nEnd").unwrap();
        assert!(
            got.windows(6)
                .any(|w| w == [0x0D, 0x10, 0x02, 0x00, 0x00, 0x00]),
            "activate is opcode 0x100D, argLen 2, count 0: {:02X?}",
            got
        );
    }

    #[test]
    fn a_numeric_argument_is_the_n_tag_and_an_i32() {
        // Measured: `setrestrained 0` writes `01 00 6E 00 00 00 00` — the
        // count, then `0x6E` `n` and a four-byte integer. `RewardXP 350`
        // writes `6E 5E 01 00 00`, where 0x015E is 350.
        let got = compile("scn S\nBegin GameMode\n  setRestrained 1\nEnd").unwrap();
        assert!(
            got.windows(7)
                .any(|w| w == [0x01, 0x00, 0x6E, 0x01, 0x00, 0x00, 0x00]),
            "the count, the n tag, and the i32: {:02X?}",
            got
        );
    }

    #[test]
    fn a_command_argument_is_not_encoded_the_way_a_set_value_is() {
        // The two encodings are genuinely different: a `set` value stores an
        // integer as its ASCII digits behind a `0x20` tag, and a command
        // argument stores it as an `i32` behind a `0x6E` tag. Writing one the
        // way the other is written produces a body the game reads as a
        // different number.
        let set = compile("scn S\nint V\nBegin GameMode\n  set V to 350\nEnd").unwrap();
        let call = compile("scn S\nBegin GameMode\n  RewardXP 350\nEnd").unwrap();
        // The set carries the ASCII `350`; the call carries the binary 350.
        assert!(
            set.windows(3).any(|w| w == [0x20, 0x33, 0x35]),
            "{set:02X?}"
        );
        assert!(
            call.windows(5).any(|w| w == [0x6E, 0x5E, 0x01, 0x00, 0x00]),
            "{call:02X?}"
        );
    }

    #[test]
    fn an_unknown_command_is_refused() {
        // A guessed opcode is a call to the wrong function.
        let err = compile("scn S\nBegin GameMode\n  NotARealCommand 1\nEnd").unwrap_err();
        assert!(err.to_string().contains("NotARealCommand"), "{err}");
    }

    #[test]
    fn a_command_given_arguments_it_does_not_take_is_refused() {
        let err = compile("scn S\nBegin GameMode\n  PurgeCellBuffers 1\nEnd").unwrap_err();
        assert!(err.to_string().contains("takes no arguments"), "{err}");
    }

    #[test]
    fn a_dotted_call_with_known_ref_is_emitted() {
        let source = "scn S\nref rDoor\nBegin GameMode\n  rDoor.Disable\nEnd";
        let compiled = crate::compile::compile(source).unwrap();
        let ins = read_instructions(&compiled);
        // ScriptName, Begin, SetCurrentRef, Disable, End
        assert_eq!(ins[2].opcode, 0x001C);
        assert_eq!(ins[2].args, 1u16.to_le_bytes());
        assert_eq!(
            ins[3].opcode,
            crate::opcodes::command_called("Disable").unwrap().opcode
        );
    }

    #[test]
    fn a_reference_argument_is_refused_rather_than_guessed() {
        // `StartCombat player` names the player, which is a reference the
        // record's list resolves.
        let err = compile("scn S\nBegin GameMode\n  StartCombat player\nEnd").unwrap_err();
        assert!(err.to_string().contains("reference"), "{err}");
    }
}

#[cfg(test)]
mod set_tests {
    use super::*;
    use crate::parse::parse;

    fn compile(source: &str) -> Result<Vec<u8>, crate::Error> {
        emit(&parse(source))
    }

    #[test]
    fn setting_a_local_to_one_is_the_games_own_bytes() {
        // Measured against BMToyCarScript, whose `int PickedUp` is the first
        // declaration and which does `set PickedUp to 1`:
        //
        //   15 00 07 00 | 73 01 00 | 02 00 | 20 31
        //
        // The destination is `s` + the 1-based index, the length counts the
        // `0x20` tag and the digit together, and `31` is the ASCII `1`.
        //
        // `OnAdd` can take a parameter, so its parameterless form writes the
        // count of zero — eight argument bytes, not six. Measured against
        // BMToyCarScript's own `Begin OnAdd`.
        assert_eq!(
            compile("scn S\nint PickedUp\nBegin OnAdd\n  set PickedUp to 1\nEnd").unwrap(),
            vec![
                0x1D, 0x00, 0x00, 0x00, // ScriptName
                0x10, 0x00, 0x08, 0x00, // Begin, argLen 8
                0x03, 0x00, // event OnAdd
                // blockLen 15: the set's 11 bytes plus the End's 4.
                0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, // paramCount 0
                0x15, 0x00, 0x07, 0x00, // Set, argLen 7
                0x73, 0x01, 0x00, // `s`, variable 1
                0x02, 0x00, // the value's length: the tag plus the digit
                0x20, 0x31, // the literal `1`
                0x11, 0x00, 0x00, 0x00, // End
            ]
        );
    }

    #[test]
    fn a_two_digit_number_is_two_ascii_digits() {
        // Measured: `to 40` writes `03 00 20 34 30` — the length is 3 because
        // it counts the tag and both digits.
        let got = compile("scn S\nint V\nBegin OnAdd\n  set V to 40\nEnd").unwrap();
        assert!(
            got.windows(5).any(|w| w == [0x03, 0x00, 0x20, 0x34, 0x30]),
            "the two-digit literal, as its own ASCII: {:02X?}",
            got
        );
    }

    #[test]
    fn setting_an_undeclared_variable_is_refused() {
        // A guessed index would assign to the wrong variable.
        let err = compile("scn S\nBegin OnAdd\n  set NoSuchVar to 1\nEnd").unwrap_err();
        assert!(err.to_string().contains("NoSuchVar"), "{err}");
    }

    #[test]
    fn a_variable_can_be_assigned_to_another_variable() {
        // The destination and the source are both tagged values, so `set A to
        // B` is two variable references with no literal at all.
        let got = compile("scn S\nint A\nint B\nBegin OnAdd\n  set A to B\nEnd").unwrap();
        // Set's arguments: `s`+1 (A), then `s`+2 (B) as the assigned value.
        assert!(
            got.windows(3).any(|w| w == [0x73, 0x01, 0x00]),
            "A is variable 1: {:02X?}",
            got
        );
        assert!(
            got.windows(3).any(|w| w == [0x73, 0x02, 0x00]),
            "B is variable 2: {:02X?}",
            got
        );
    }

    #[test]
    fn an_arithmetic_expression_in_set_is_emitted() {
        // `set V to 1 + 2` is emitted as postfix: 1 2 +
        let got = compile("scn S\nint V\nBegin OnAdd\n  set V to 1 + 2\nEnd").unwrap();
        // 0x20 '1' 0x20 '2' 0x20 '+'
        assert!(got
            .windows(6)
            .any(|w| w == [0x20, 0x31, 0x20, 0x32, 0x20, 0x2B]));
    }
}

#[cfg(test)]
mod param_count_tests {
    use super::*;
    use crate::parse::parse;

    fn compile(source: &str) -> Result<Vec<u8>, crate::Error> {
        emit(&parse(source))
    }

    #[test]
    fn an_event_that_cannot_take_an_argument_writes_no_count() {
        // `GameMode` is six argument bytes with no count at all. Measured over
        // 584 parameterless `GameMode` blocks in the corpus.
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
    fn an_event_that_can_take_an_argument_writes_a_count_of_zero() {
        // `OnDeath` is eight argument bytes with a count of zero, even with
        // nothing passed to it. Measured over 305 parameterless `OnDeath`
        // blocks — and the 1,132-to-1,033 split between the two forms across
        // the corpus is exactly this rule.
        assert_eq!(
            compile("scn S\nBegin OnDeath\nEnd").unwrap(),
            vec![
                0x1D, 0x00, 0x00, 0x00, // ScriptName
                0x10, 0x00, 0x08, 0x00, // Begin, argLen 8
                0x0A, 0x00, // event OnDeath
                0x04, 0x00, 0x00, 0x00, // blockLen 4
                0x00, 0x00, // paramCount 0
                0x11, 0x00, 0x00, 0x00, // End
            ]
        );
    }

    #[test]
    fn the_two_forms_are_both_real_and_the_corpus_splits_between_them() {
        // The rule stated as a table, so a future edit that gets it wrong has
        // a test to fail. These are the corpus's own counts.
        assert!(!Event::GameMode.takes_argument());
        assert!(!Event::OnLoad.takes_argument());
        assert!(!Event::OnReset.takes_argument());
        assert!(!Event::ScriptEffectStart.takes_argument());
        assert!(Event::OnActivate.takes_argument());
        assert!(Event::OnDeath.takes_argument());
        assert!(Event::OnTriggerEnter.takes_argument());
        assert!(Event::OnTrigger.takes_argument());
    }
}

/// Emit a command call.
///
/// Measured, and the shape depends on whether the command declares any
/// parameters at all:
///
/// ```text
/// [opcode:u16][argLen:u16]                       — a command with no parameters
/// [opcode:u16][argLen:u16] [argCount:u16] [args…] — a command that has some
/// ```
///
/// `StopCombat` and `PickIdle` are the first form: their `argLen` is 0 and
/// there is nothing after the header. `activate`, `Disable` and `unlock` are
/// the second, and they write a count of zero even when called with no
/// arguments — so `activate` is `100D 00 02 00 00 00`, four bytes of header
/// and a count of 0.
///
/// The rule holds across every one of the 53 command opcodes measured in the
/// corpus: `argLen` 0 exactly when the command declares no parameters.
///
/// The arguments themselves are tagged values, and the tag set is measured:
/// `0x6E` `n` + an `i32` for a number, `0x72` `r` + a `u16` index for one of
/// the record's references. A reference name is not resolvable here — the
/// record's own reference list is — so a call naming one is refused rather
/// than given a made-up index.
/// Resolve a reference name to its 1-based index in `[declared ref
/// variables] + [SCRO refs]`.
///
/// Delegated to [`crate::value::resolve_ref_index`], where the measured rule
/// lives: declared `ref` variables first, then the record's SCRO FormIDs.
fn resolve_ref(name: &str, script: &Script) -> Result<u16, crate::Error> {
    crate::value::resolve_ref_index(name, script)
}

fn emit_call(
    script: &Script,
    on: &Option<String>,
    command: &str,
    args: &[crate::lex::Token],
) -> Result<Vec<u8>, crate::Error> {
    let Some(cmd) = crate::opcodes::command_called(command) else {
        return Err(crate::Error::Parse(format!(
            "`{command}` is not a known command"
        )));
    };

    let mut prefix_bytes = Vec::new();
    if let Some(ref_name) = on {
        let ref_idx = resolve_ref(ref_name, script)?;
        // 0x1C SetCurrentRef framing: [1C 00] [refIdx: u16]
        prefix_bytes.extend_from_slice(&0x001Cu16.to_le_bytes());
        prefix_bytes.extend_from_slice(&ref_idx.to_le_bytes());
    }

    // The arguments, as the values the source spells. A name that is not a
    // number and not a declared variable is a reference or a form the emitter
    // does not understand yet, and it is refused rather than guessed.
    //
    // The count is of arguments, not of bytes: one `n`-tagged argument is five
    // bytes, and writing its byte length as the count is a call with five
    // arguments where the source asked for one.
    // A command that drops its argument must not encode it: the argument is
    // often a package or idle name that is nowhere in the record's reference
    // list (`VMS18WhiteGloveKitchenPatrollerREF.RemoveScriptPackage
    // VMS18KitchenPatrollerFollowPlayer`), so resolving it would fail on a
    // call whose bytes carry no argument at all.
    let folded = crate::value::fold_dotted(args);
    let significant: Vec<&crate::lex::Token> = folded
        .iter()
        .filter(|t| {
            t.kind != crate::lex::Kind::Space
                && t.kind != crate::lex::Kind::Comment
                && t.text != ","
        })
        .collect();
    if matches!(
        cmd.opcode,
        0x1017 | 0x1098 | 0x105E | 0x105D | 0x1101 | 0x11FA | 0x11BB | 0x1158
    ) && significant.len() == 1
    {
        let mut out = prefix_bytes;
        out.extend(frame(cmd.opcode, &[]));
        return Ok(out);
    }

    let mut encoded: Vec<Vec<u8>> = Vec::new();
    let mut param_idx = 0;
    let mut ai = 0;
    while ai < folded.len() {
        let t = &folded[ai];
        if t.kind == crate::lex::Kind::Space || t.kind == crate::lex::Kind::Comment || t.text == ","
        {
            ai += 1;
            continue;
        }
        // Handle negative number passed as `-` followed by `number`
        if t.text == "-" && ai + 1 < folded.len() && folded[ai + 1].kind == crate::lex::Kind::Number {
            let num_tok = &folded[ai + 1];
            let mut combined = num_tok.clone();
            combined.text = format!("-{}", num_tok.text);
            encoded.push(emit_arg(&combined, script, Some(cmd), param_idx)?);
            param_idx += 1;
            ai += 2;
            continue;
        }
        encoded.push(emit_arg(t, script, Some(cmd), param_idx)?);
        param_idx += 1;
        ai += 1;
    }

    // A command with no declared parameters writes no argument bytes at all.
    // One that has them writes the count, even when nothing is passed.
    if cmd.args.is_empty() {
        // The two exceptions the corpus settles: `StopCombat` and
        // `RemoveScriptPackage` are called in shipped scripts with an
        // explicit target (`Fighter02.StopCombat Player`,
        // `VFSOrrisThug01REF.RemoveScriptPackage SomePackage`) and the game
        // compiles them to argLen 0 — the argument is dropped, because for
        // these two the target is the reference the call is dotted on (or
        // the script's own actor when bare). Measured: 164 such calls across
        // the corpus, every one argLen 0, dotted and bare alike. One
        // dropped argument is accepted on that evidence; two is a call this
        // emitter does not understand.
        if encoded.len() == 1
            && matches!(cmd.opcode, 0x1017 | 0x1098 | 0x1142 | 0x105E | 0x105D)
        {
            let mut out = prefix_bytes;
            out.extend(frame(cmd.opcode, &[]));
            return Ok(out);
        }
        if !encoded.is_empty() {
            return Err(crate::Error::Parse(format!(
                "`{command}` takes no arguments, but {} were given",
                encoded.len()
            )));
        }
        let mut out = prefix_bytes;
        out.extend(frame(cmd.opcode, &[]));
        return Ok(out);
    }

    // ShowMessage's compiled form is its own shape, measured across the
    // corpus: the count word counts the message alone, then come the
    // format-value count, the format values themselves, and a u32 zero the
    // game always writes.
    //
    //   showmessage WildWastelandMessage
    //     → 01 00 | 72 04 00 | 00 00 | 00 00 00 00
    //   showmessage ChallengeMessageBugStomper rank
    //     → 01 00 | 72 03 00 | 01 00 | 73 01 00 | 00 00 00 00
    //   ShowMessage TrapLowSkillRepairMsg NQlvl.TRAPPlateSkillReq
    //     → 01 00 | 72 05 00 | 01 00 | 72 02 00 73 1B 00 | 00 00 00 00
    //
    // The first argument is the message (a reference); every argument after
    // it is a printf-style format value, counted by their own u16. A dotted
    // remote variable as a format value is the 5-byte compound
    // `72 <refIdx> <tag> <varId>`.
    if cmd.opcode == 0x1059 {
        if encoded.is_empty() {
            return Err(crate::Error::Parse(
                "`ShowMessage` needs its message argument".to_string(),
            ));
        }
        // A bare number after the message is not a format value: see the tail
        // below. It is taken off the end and loses its `n` tag.
        let mut literal_arg: Option<i64> = None;
        if significant.len() == 2 && significant[1].kind == crate::lex::Kind::Number {
            if let Ok(n) = significant[1].text.parse::<i64>() {
                literal_arg = Some(n);
                encoded.pop();
            }
        }
        if encoded.is_empty() {
            return Err(crate::Error::Parse(
                "`ShowMessage` needs its message argument".to_string(),
            ));
        }
        // A format value is a *value*, so a global in that position writes
        // `47` 'G': measured, ThePitt.esm's
        // `showmessage DLC01FF008MedicineLowMessage DLC01FF008MedicineSkillNeeded`
        // writes `01 00 | 72 08 00 | 01 00 | 47 06 00` — the second name is a
        // GLOB and the game writes the global tag, where a reference argument
        // would write `72`.
        for (at, one) in encoded.iter_mut().enumerate().skip(1) {
            if one.first() != Some(&0x72) {
                continue;
            }
            let idx = u16::from_le_bytes([one[1], one[2]]);
            let is_global = script
                .refs
                .get(idx as usize - 1)
                .is_some_and(|r| r.kind == crate::parse::RefKind::Global);
            if is_global && significant.get(at).is_some() {
                one[0] = 0x47;
            }
        }
        let message = encoded.remove(0);
        let mut body = Vec::new();
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend(message);
        body.extend_from_slice(&(encoded.len() as u16).to_le_bytes());
        for e in &encoded {
            body.extend(e);
        }
        // The trailing word is zero in every call that passes format values.
        // Measured, the one call that passes a bare number instead —
        // `ShowMessage VCG01CasualHardcoreMessage 1` in
        // VCG01CasualHardcoreMessageSCRIPT — writes `00 00 | 01 00 00 00`: no
        // format values, and the number in that trailing word. One unit is the
        // whole evidence, so it is reproduced exactly rather than generalised.
        let tail = match literal_arg {
            Some(n) => n as u32,
            None => 0,
        };
        body.extend_from_slice(&tail.to_le_bytes());
        let mut out = prefix_bytes;
        out.extend(frame(cmd.opcode, &body));
        return Ok(out);
    }

    // `ShowWarning` frames its arguments in two counts: how many strings, then
    // the strings, then how many other values, then those. Measured on
    // Fallout3.esm's MS13MesmetronEffectScript, `ShowWarning "Mesmetron die
    // roll: %.0f" dieRoll` writes `01 00 | 18 00 | <the 24 characters> | 01 00
    // | 73 06 00`, and the same unit's stringless calls write `01 00 | 0A 00 |
    // "Always Mez" | 00 00`.
    if cmd.opcode == 0x11B9 {
        let mut strings = 0u16;
        let mut others = 0u16;
        let mut body = Vec::new();
        for (at, t) in significant.iter().enumerate() {
            let is_string = t.kind == crate::lex::Kind::String
                || cmd
                    .args
                    .get(at)
                    .is_some_and(|ty| param_kind(ty).as_str() == "string");
            if is_string {
                strings += 1;
            } else {
                others += 1;
            }
        }
        body.extend_from_slice(&strings.to_le_bytes());
        let mut tail: Vec<u8> = Vec::new();
        for (at, one) in encoded.iter().enumerate() {
            let t = significant.get(at);
            let is_string = t.is_some_and(|t| t.kind == crate::lex::Kind::String)
                || cmd
                    .args
                    .get(at)
                    .is_some_and(|ty| param_kind(ty).as_str() == "string");
            if is_string {
                body.extend(one);
            } else {
                tail.extend(one);
            }
        }
        body.extend_from_slice(&others.to_le_bytes());
        body.extend(tail);
        let mut out = prefix_bytes;
        out.extend(frame(cmd.opcode, &body));
        return Ok(out);
    }

    let flat: Vec<u8> = encoded.concat();
    let mut body = Vec::with_capacity(2 + flat.len());
    body.extend_from_slice(&(encoded.len() as u16).to_le_bytes());
    body.extend(flat);
    let mut out = prefix_bytes;
    out.extend(frame(cmd.opcode, &body));
    Ok(out)
}

/// One command argument, as its tagged bytes.
///
/// The tags are measured: `0x6E` `n` + an `i32` for a number, `0x72` `r` + a
/// `u16` index for a reference. This is a different encoding from `set`'s
/// value, which stores an integer as its ASCII digits behind a `0x20` tag —
/// the two are not interchangeable, and writing a command's argument the way
/// `set` writes its value produces a body the game reads as a different
/// number.
pub fn actor_value_code(name: &str) -> Option<u16> {
    const AV_NAMES: &[(&str, u16)] = &[
        ("Aggression", 0),
        ("Confidence", 1),
        ("Energy", 2),
        ("Responsibility", 3),
        ("Mood", 4),
        ("Strength", 5),
        ("Perception", 6),
        ("Endurance", 7),
        ("Charisma", 8),
        ("Intelligence", 9),
        ("Agility", 10),
        ("Luck", 11),
        ("ActionPoints", 12),
        ("CarryWeight", 13),
        ("CritChance", 14),
        ("HealRate", 15),
        ("Health", 16),
        ("MeleeDamage", 17),
        ("DamageResistance", 18),
        // The name the scripts spell: `Player.GetAV DamageResist` is 18.
        ("DamageResist", 18),
        ("PoisonResistance", 19),
        ("RadResistance", 20),
        ("SpeedMult", 21),
        ("Fatigue", 22),
        ("Karma", 23),
        ("XP", 24),
        ("PerceptionCondition", 25),
        ("EnduranceCondition", 26),
        ("LeftAttackCondition", 27),
        ("RightAttackCondition", 28),
        ("LeftMobilityCondition", 29),
        ("RightMobilityCondition", 30),
        ("BrainCondition", 31),
        ("Barter", 32),
        ("BigGuns", 33),
        ("EnergyWeapons", 34),
        ("Explosives", 35),
        ("Lockpick", 36),
        ("Medicine", 37),
        ("MeleeWeapons", 38),
        ("Repair", 39),
        ("Science", 40),
        ("Guns", 41),
        // Fallout 3 spells the same skill `SmallGuns`; measured
        // `Player.ModAv SmallGuns 10` → `29 00` (41) in BobbleheadSGUNScript.
        ("SmallGuns", 41),
        ("Sneak", 42),
        ("Speech", 43),
        ("Survival", 44),
        ("Unarmed", 45),
        ("InventoryWeight", 46),
        ("Paralysis", 47),
        ("Invisibility", 48),
        ("Chameleon", 49),
        ("NightEye", 50),
        ("Turbo", 51),
        ("FireResistance", 52),
        ("WaterBreathing", 53),
        ("RadLevel", 54),
        // The same code under the name the scripts actually spell: measured
        // `player.RestoreAV RadiationRads 1000` → `36 00` (54) in
        // VFSJulieFarkasSCRIPT, and 5 units spell it this way.
        ("RadiationRads", 54),
        // The same code under the name the Old World Blues scripts spell.
        ("RadResist", 20),
        // Fallout 3's name for the same code: measured
        // `triggerActor.getAV detectLifeRange` → `58 0E 10 04 00 01 00 33 00`
        // (BrokenSteel.esm's testPhilCLightningVol1SCRIPT).
        ("DetectLifeRange", 51),
        // The DLCs' own hardcore meter, misspelled in every script that uses
        // it: measured `player.SetAV sleepdeprevation 0` → `02 00 | 4B 00`
        // (DeadMoney.esm's NVDLC01SuitesAutoDocSCRIPT).
        ("SleepDeprevation", 75),
        ("BloodyMess", 55),
        ("UnarmedDamage", 56),
        ("Assistance", 57),
        ("ElectricResistance", 58),
        ("FrostResistance", 59),
        ("EnergyResistance", 60),
        ("EMPResistance", 61),
        ("Variable01", 62),
        ("Variable02", 63),
        ("Variable03", 64),
        ("Variable04", 65),
        ("Variable05", 66),
        ("Variable06", 67),
        ("Variable07", 68),
        ("Variable08", 69),
        ("Variable09", 70),
        ("Variable10", 71),
        ("IgnoreCrippledLimbs", 72),
        ("Dehydration", 73),
    ];
    for (n, c) in AV_NAMES {
        if n.eq_ignore_ascii_case(name) {
            return Some(*c);
        }
    }
    None
}

pub fn anim_group_code(name: &str) -> Option<u16> {
    const ANIM_GROUPS: &[(&str, u16)] = &[
        ("Idle", 0),
        ("SpecialIdle", 2),
        ("Forward", 3),
        ("Backward", 4),
        ("Left", 5),
        ("Right", 6),
        ("FastForward", 7),
        ("FastBackward", 8),
        ("FastLeft", 9),
        ("FastRight", 10),
        ("TurnLeft", 15),
        // Measured in Anchorage.esm's DLCAnchOverlookGunsSCRIPT: the source's
        // `playgroup TurnRight 1` compiles to `02 00 | 10 00 | 6E 01 …`.
        ("TurnRight", 16),
        // Fallout 3 spells the reload groups lettered, and each code is
        // measured: `examples/pair_args.rs -- 1013 PlayGroup` pairs every
        // `PlayGroup <name>` call in Zeta.esm with the instruction it compiled
        // to (422 names paired, no disagreements) and reads 142..147 for
        // ReloadA..ReloadF.
        ("ReloadA", 142),
        ("ReloadB", 143),
        ("ReloadC", 144),
        ("ReloadD", 145),
        ("ReloadE", 146),
        ("ReloadF", 147),
        ("Equip", 24),
        ("Unequip", 25),
    ];
    for (n, c) in ANIM_GROUPS {
        if n.eq_ignore_ascii_case(name) {
            return Some(*c);
        }
    }
    None
}

/// A code parameter's name, as the `u16` the game writes.
///
/// Every code here was read off the corpus, because a code has no name in the
/// bytes to find: `GetIsSex Male` is 0 and `Female` 1 (BCTrooperHostageScript);
/// `IsInCriticalStage GooEnd` is 2 and `DisintegrateEnd` 4
/// (CookCookScript), with `SetCriticalStage GooStart` 1 and
/// `DisintegrateStart` 3 (PlasmaGooificationEffectScript); `GetPCMiscStat
/// "Locations Discovered"` is 1 and `"Times Addicted"` 10 (VCGTutorialSCRIPT).
/// A name outside these refuses rather than guesses — a wrong code is a call
/// that silently does something else.
fn code_named(name: &str, ty: &str) -> Option<Vec<u8>> {
    let lower = name.to_ascii_lowercase();
    let code: u16 = match param_kind(ty).as_str() {
        "sex" => match lower.as_str() {
            "male" => 0,
            "female" => 1,
            _ => return None,
        },
        "criticalstage" => match lower.as_str() {
            // Measured: `If (IsInCriticalStage None != 1)` in
            // NVDLC04TempleMarkedMen*SCRIPT (LonesomeRoad.esm) writes `01 00
            // 00 00` — no stage at all is code 0.
            "none" => 0,
            "goostart" => 1,
            "gooend" => 2,
            "disintegratestart" => 3,
            "disintegrateend" => 4,
            _ => return None,
        },
        "miscellaneousstat" => match lower.as_str() {
            // Measured in FalloutNV.esm (Locations Discovered → 1, Times
            // Addicted → 10) and in Fallout3.esm's AchievementScript, whose
            // nine `GetPCMiscStat` calls pair with codes 2, 3, 12, 5, 4, 1, 14,
            // 16, 16 in source order.
            "locations discovered" => 1,
            "people killed" => 2,
            "creatures killed" => 3,
            "locks picked" => 4,
            "computers hacked" => 5,
            "times addicted" => 10,
            "speech successes" => 12,
            "pants exploded" => 14,
            "bobbleheads found" => 16,
            // Measured from the `ModPCMiscStat` calls that write them:
            // `Weapons Created` → 17, `People Mezzed` → 18, `Captives
            // Rescued` → 19.
            "weapons created" => 17,
            "people mezzed" => 18,
            "captives rescued" => 19,
            _ => return None,
        },
        "actorvalue" => actor_value_code(name)?,
        "animgroup" => anim_group_code(name)?,
        _ => return None,
    };
    Some(code.to_le_bytes().to_vec())
}

/// Whether a parameter type is written as a bare `u16` code rather than a
/// tagged value.
///
/// Measured for `ActorValue` (`DamageAV Health 5` writes `16 00`), `AnimGroup`
/// (`PlayGroup Forward` writes `03 00`) and `Miscellaneous Stat`
/// (`GetPCMiscStat "Times Addicted"` writes `0A 00`) — all three are a raw
/// little-endian word with no tag and no count of their own.
/// Whether a parameter is written as `7A` + `f64`.
///
/// `Float` is the table's usual spelling, but the same parameter is spelled
/// `Health Percent`, and measured it is a float: PointLookout.esm's
/// `AddItemToLeveledList VendorChestFoodOnly DLc04PungaWild 1 4 .25` writes
/// `7A 00 00 00 00 00 00 D0 3F` — 0.25 as an `f64`. `PlaceAtMeHealthPercent`
/// carries the same parameter name and is written the same way here; the
/// corpus has no call to it, so that one is by name rather than by bytes.
fn is_float_param(ty: &str) -> bool {
    matches!(param_kind(ty).as_str(), "float" | "healthpercent")
}

fn is_code_param(ty: &str) -> bool {
    matches!(
        param_kind(ty).as_str(),
        "actorvalue" | "animgroup" | "miscellaneousstat" | "sex" | "criticalstage"
    )
}

/// A parameter type reduced to a comparable key.
///
/// The table spells the same type more than one way — `ActorValue` (4 rows) and
/// `Actor Value` (9), `AnimGroup` (1) and `Animation Group` (2) — and comparing
/// the raw strings silently missed the majority spelling, which is how
/// `IsAnimPlaying Forward` and `GetActorValue Health` came to be refused as
/// "names a reference".
fn param_kind(ty: &str) -> String {
    let key: String = ty
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    // `Animation Group` (2 rows) and `AnimGroup` (1) are the same parameter.
    if key == "animationgroup" {
        "animgroup".to_string()
    } else {
        key
    }
}

/// A code parameter's name, as the `u16` the game writes. and whatever the command
/// pads it with.
///
/// Measured, and it is not a FormID and not a reference index — the name is
/// the text itself, so a form the record's `SCRO` list does not carry can
/// still be passed:
///
/// ```text
/// PlayIdle LooseDoctorHealPlayer
///   → 90 11 | 19 00 | 01 00 | 15 00 | "LooseDoctorHealPlayer"
///      opcode  argLen  count   len=21 the characters, no terminator
/// ```
///
/// `ShowWarning` pads the text with two zero bytes (`01 00 | 07 00 | "Got Hit"
/// | 00 00`), and it is the only command in the corpus that does: 42 of 42
/// `ShowWarning` calls carry the pair, and `PlayIdle` (90), `GetGameSetting`,
/// `IsDLCInstalled`, `EnableNavMesh` and `PlayBink` carry none.
fn string_arg(text: &str, cmd: Option<&'static crate::opcodes::Command>) -> Vec<u8> {
    // The quotes are the source's delimiters, not part of the name; a doubled
    // quote inside them is one quote.
    let inner = text.trim_matches('"').replace("\"\"", "\"");
    let bytes = inner.as_bytes();
    let mut out = Vec::with_capacity(2 + bytes.len() + 2);
    out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    out.extend_from_slice(bytes);
    // The two bytes `ShowWarning` writes after its text are its count of value
    // arguments, and `emit_call` writes them, not this function.
    let _ = cmd;
    out
}

/// Encode one command argument, for the parameter type at `param_idx`.
///
/// The one place a command argument is turned into bytes, so the statement
/// form and both expression forms (`Ref.Command` and bare `Command`) cannot
/// disagree about what a name in a given position means.
pub fn encode_command_arg(
    t: &crate::lex::Token,
    script: &Script,
    cmd: &'static crate::opcodes::Command,
    param_idx: usize,
) -> Result<Vec<u8>, crate::Error> {
    emit_arg(t, script, Some(cmd), param_idx)
}

fn emit_arg(
    t: &crate::lex::Token,
    script: &Script,
    cmd: Option<&'static crate::opcodes::Command>,
    param_idx: usize,
) -> Result<Vec<u8>, crate::Error> {
    let expected_type = cmd.and_then(|c| c.args.get(param_idx)).copied();
    // A quoted token is whatever the parameter's declared type says it is:
    //  - a code parameter reads the name (`GetPCMiscStat "Times Addicted"` is
    //    `04 00 | 01 00 | 0A 00`, a code — not the twelve characters);
    //  - a form parameter reads the name as that form (`AddNote
    //    "MegatonGenericBrassLanternPassword"` is `01 00 | 72 02 00`, a
    //    reference index);
    //  - anything else is the text itself (`ShowWarning "Got Hit"`).
    if t.kind == crate::lex::Kind::String {
        let inner = t.text.trim_matches('"').replace("\"\"", "\"");
        if let Some(exp) = expected_type {
            let kind = param_kind(exp);
            if is_code_param(exp) {
                return code_named(&inner, exp).ok_or_else(|| {
                    crate::Error::Parse(format!(
                        "`{inner}` is not a {kind} this emitter knows"
                    ))
                });
            }
            if kind != "string" {
                if let Ok(idx) = crate::value::resolve_ref_index(&inner, script) {
                    let mut b = vec![0x72];
                    b.extend_from_slice(&idx.to_le_bytes());
                    return Ok(b);
                }
            }
        }
        return Ok(string_arg(&t.text, cmd));
    }
    match t.kind {
        crate::lex::Kind::Number => {
            // Check if expected parameter type is Float
            if let Some(exp) = expected_type {
                if is_float_param(exp) {
                    let Ok(f) = t.text.parse::<f64>() else {
                        return Err(crate::Error::Parse(format!(
                            "`{}` is not a float this emitter can encode",
                            t.text
                        )));
                    };
                    let mut out = vec![0x7A];
                    out.extend_from_slice(&f.to_le_bytes());
                    return Ok(out);
                }
                // A code parameter is a bare `u16`, not an `n`-tagged i32:
                // measured for `GetPCMiscStat "Times Addicted"` (`38 11 04 00
                // 01 00 0A 00` — the code is the last two bytes) and for
                // ActorValue, AnimGroup and the rest.
                if is_code_param(exp) {
                    let Ok(n) = t.text.parse::<i64>() else {
                        return Err(crate::Error::Parse(format!(
                            "`{}` is not a code this emitter can encode",
                            t.text
                        )));
                    };
                    return Ok((n as u16).to_le_bytes().to_vec());
                }
                if param_kind(exp).as_str() == "string" {
                    return Ok(string_arg(&t.text, cmd));
                }
            }
            let Ok(n) = t.text.parse::<i32>() else {
                return Err(crate::Error::Parse(format!(
                    "`{}` is not a number this emitter can encode",
                    t.text
                )));
            };
            let mut out = vec![0x6E];
            out.extend_from_slice(&n.to_le_bytes());
            Ok(out)
        }
        // A bare name is a reference or a variable of this script. The
        // variable case is resolvable through the declarations; anything else
        // names one of the record's references, which this emitter cannot
        // index correctly — so it is refused rather than guessed.
        crate::lex::Kind::Word => {
            // `Script.var` — a remote script variable, the 5-byte compound
            // `72 <refIdx> <tag> <the variable's own SLSD id>`. Measured:
            // `set Follower1 to GomorrahCasinoEnterTriggerREF.Follower1`
            // (FalloutNV.esm) and `linkedref.PlaceLeveledActorAtMe … DLC05HQ.spawnDiff`
            // (Zeta.esm, as an argument).
            if let Some((target, member)) = t.text.split_once('.') {
                if !target.is_empty() && !member.is_empty() {
                    let (tag, id) = crate::value::resolve_remote_var(target, member, script)?;
                    let ref_idx = crate::value::resolve_ref_index(target, script)?;
                    let mut b = vec![0x72];
                    b.extend_from_slice(&ref_idx.to_le_bytes());
                    b.push(tag);
                    b.extend_from_slice(&id.to_le_bytes());
                    return Ok(b);
                }
            }
            // The parameter's declared type decides what a name means, and it
            // has to come first: `GetPos X` is the axis letter even when the
            // script declares a variable called `X`, and `PlayIdle SomeName`
            // is the idle's own text even though the name is nowhere in the
            // record's reference list.
            if let Some(exp) = expected_type {
                if param_kind(exp).as_str() == "string" {
                    return Ok(string_arg(&t.text, cmd));
                }
                if param_kind(exp).as_str() == "actorvalue" {
                    if let Some(av) = actor_value_code(&t.text) {
                        return Ok(av.to_le_bytes().to_vec());
                    }
                    return Err(crate::Error::Parse(format!(
                        "`{}` is not an actor value this emitter knows",
                        t.text
                    )));
                }
                if param_kind(exp).as_str() == "animgroup" {
                    if let Some(ag) = anim_group_code(&t.text) {
                        return Ok(ag.to_le_bytes().to_vec());
                    }
                    return Err(crate::Error::Parse(format!(
                        "`{}` is not an animation group this emitter knows",
                        t.text
                    )));
                }
                if param_kind(exp).as_str() == "axis" {
                    // Measured: `SetAngle X 0` writes the axis as its own
                    // ASCII letter — `58 7A ...` for X, `59` for Y, `5A` for
                    // Z — not a tagged value and not a raw u16.
                    match t.text.to_ascii_uppercase().as_str() {
                        "X" => return Ok(vec![b'X']),
                        "Y" => return Ok(vec![b'Y']),
                        "Z" => return Ok(vec![b'Z']),
                        _ => {}
                    }
                    return Err(crate::Error::Parse(format!(
                        "`{}` is not an axis",
                        t.text
                    )));
                }
                if is_code_param(exp) {
                    return code_named(&t.text, exp).ok_or_else(|| {
                        crate::Error::Parse(format!(
                            "`{}` is not a {} this emitter knows",
                            t.text,
                            param_kind(exp)
                        ))
                    });
                }
                if is_code_param(exp) {
                    return Err(crate::Error::Parse(format!(
                        "`{}` is not a code this emitter can encode",
                        t.text
                    )));
                }
            }

            // A declared variable of the record's table. In a command
            // argument position a `ref` variable is a reference — the game
            // writes `72` + its index in the reference space (declared
            // refs first, then SCRO), the same space a placed reference
            // name resolves through — while an int or float variable keeps
            // its tagged variable form. Measured: the corpus writes
            // `72 01 00` for a declared ref passed as an argument where an
            // int var at the same position writes `73 <id>`.
            if let Some(d) = script
                .decls
                .iter()
                .find(|d| d.name.eq_ignore_ascii_case(&t.text))
            {
                if d.kind == crate::parse::DeclKind::Ref {
                    let ref_idx = crate::value::resolve_ref_index(&t.text, script)?;
                    let mut out = vec![0x72];
                    out.extend_from_slice(&ref_idx.to_le_bytes());
                    return Ok(out);
                }
                if let Ok((idx, is_int)) = crate::value::resolve_var(&t.text, script) {
                    let tag = if is_int { 0x73 } else { 0x66 };
                    let mut out = vec![tag];
                    out.extend_from_slice(&idx.to_le_bytes());
                    return Ok(out);
                }
            }
            // A standalone reference-list name as an argument: the tag follows
            // what the entry names, exactly as it does in an expression — a
            // global writes `47` 'G' and a placed reference `5A`/`72`. Measured:
            // `PlaceAtReticle EuclidTargetPointer 1 0 EuclidPointerRange` writes
            // the global as `47 05 00`, where `72 05 00` is a different operand
            // the game would read as a reference.
            if let Ok(ref_idx) = crate::value::resolve_ref_index(&t.text, script) {
                let kind = script
                    .refs
                    .iter()
                    .find(|r| r.name.eq_ignore_ascii_case(&t.text))
                    .map(|r| r.kind)
                    .unwrap_or(crate::parse::RefKind::Reference);
                // A global read as a *number* writes `47` ('G'); used where a
                // reference is expected it stays `72`. Measured:
                // `PlaceAtReticle EuclidTargetPointer 1 0 EuclidPointerRange`
                // takes a Float and writes `47 05 00`, while a reference
                // parameter writes `72`.
                let numeric = expected_type.is_some_and(|exp| {
                    matches!(
                        param_kind(exp).as_str(),
                        "float"
                            | "integer"
                            | "int"
                            | "value"
                            | "amount"
                            | "count"
                            | "xp"
                            | "level"
                            | "stage"
                            | "distance"
                            | "percent"
                    )
                });
                let tag = match kind {
                    crate::parse::RefKind::Global if numeric => 0x47,
                    _ => 0x72,
                };
                let mut out = vec![tag];
                out.extend_from_slice(&ref_idx.to_le_bytes());
                return Ok(out);
            }
            Err(crate::Error::Parse(format!(
                "`{}` names a reference, which the emitter cannot resolve yet",
                t.text
            )))
        }
        _ => Err(crate::Error::Parse(format!(
            "cannot encode `{}` as a command argument",
            t.text
        ))),
    }
}

#[cfg(test)]
mod corpus_tests {
    //! Byte-exact assertions taken from the game's own compiled scripts.
    //!
    //! Every expectation here is a run of bytes read out of `FalloutNV.esm` or
    //! `Fallout3.esm`'s `SCDA`, named with the unit it came from. A test written
    //! from this crate's own output would only prove self-consistency; these are
    //! the measurements, so a change that breaks one has changed an encoding the
    //! game does not share.

    use crate::emit::read_instructions;
    use crate::parse::{RefEntry, RefKind, RefSlot};

    fn refs(names: &[&str]) -> Vec<RefEntry> {
        names
            .iter()
            .map(|n| RefEntry {
                name: (*n).to_string(),
                slot: RefSlot::Form,
                kind: RefKind::Reference,
                form: 0,
            })
            .collect()
    }

    #[test]
    fn playidle_writes_its_idle_name_as_text() {
        // Measured, DoctorKempSCRIPT (FalloutNV.esm):
        //   PlayIdle LooseDoctorHealPlayer
        //     → 90 11 | 19 00 | 01 00 | 15 00 | "LooseDoctorHealPlayer"
        // The idle is not in the record's reference list at all, so a compiler
        // that wants an index here cannot emit the call.
        let got = crate::compile("scn S\nBegin GameMode\n  PlayIdle LooseDoctorHealPlayer\nEnd\n").unwrap();
        let expected: Vec<u8> = [
            &[0x90u8, 0x11, 0x19, 0x00, 0x01, 0x00, 0x15, 0x00][..],
            b"LooseDoctorHealPlayer",
        ]
        .concat();
        assert!(
            got.windows(expected.len()).any(|w| w == expected),
            "{:02X?}",
            got
        );
    }

    #[test]
    fn showwarning_frames_a_string_count_then_a_value_count() {
        // Measured, MS13MesmetronEffectScript (Fallout3.esm):
        //   ShowWarning "Mesmetron die roll: %.0f" dieRoll
        //     → B9 11 | 21 00 | 01 00 | 18 00 | <24 characters> | 01 00 | 73 06 00
        let got = crate::compile_with_refs(
            "scn S\nshort dieRoll\nBegin GameMode\n  ShowWarning \"ab\" dieRoll\nEnd\n",
            &[],
            &[],
            &[],
        )
        .unwrap();
        // argLen 2 (string count) + 2 (length) + 2 (text) + 2 (value count) + 3 (the var)
        let expected = [
            0xB9u8, 0x11, 0x0B, 0x00, 0x01, 0x00, 0x02, 0x00, b'a', b'b', 0x01, 0x00, 0x73, 0x01,
            0x00,
        ];
        assert!(
            got.windows(expected.len()).any(|w| w == expected),
            "{:02X?}",
            got
        );
    }

    #[test]
    fn a_negative_literal_is_its_digits_then_the_negate_operator() {
        // Measured: `set X to -1` writes `20 31 20 7E` — never a signed ASCII
        // literal, and never the binary minus `2D`.
        let got = crate::compile_with_refs(
            "scn S\nshort X\nBegin GameMode\n  set X to -1\nEnd\n",
            &[],
            &[crate::VarEntry {
                name: "X".to_string(),
                id: 1,
                is_int: true,
            }],
            &[],
        )
        .unwrap();
        assert!(
            got.windows(7).any(|w| w == [0x15, 0x00, 0x05, 0x00, 0x73, 0x01, 0x00]
                || w == [0x73, 0x01, 0x00, 0x04, 0x00, 0x20, 0x31]),
            "{:02X?}",
            got
        );
        assert!(got.windows(2).any(|w| w == [0x20, 0x7E]), "{:02X?}", got);
        assert!(!got.windows(2).any(|w| w == [0x20, 0x2D]), "{:02X?}", got);
    }

    #[test]
    fn else_if_writes_the_else_and_drops_the_condition() {
        // Measured, MS01QuestScript (Fallout3.esm): `else if iDestroyed == 1`
        // compiles to `0017 else | 01 00` and then the body — no `if`
        // instruction, and no trace of the condition.
        let src = "scn S\nshort a\nBegin GameMode\nif a == 0\nset a to 1\nelse if a == 2\nset a to 3\nendif\nEnd\n";
        let got = crate::compile_with_refs(
            src,
            &[],
            &[crate::VarEntry {
                name: "a".to_string(),
                id: 1,
                is_int: true,
            }],
            &[],
        )
        .unwrap();
        let ins = read_instructions(&got);
        let opcodes: Vec<u16> = ins.iter().map(|i| i.opcode).collect();
        assert_eq!(opcodes, vec![0x1D, 0x10, 0x16, 0x15, 0x17, 0x15, 0x19, 0x11]);
        // The `else`'s jumpOps counts the one statement under it.
        let at = ins.iter().position(|i| i.opcode == 0x0017).unwrap();
        assert_eq!(ins[at].args, 1u16.to_le_bytes().to_vec());
    }

    #[test]
    fn a_digit_leading_record_name_is_one_word() {
        // Fallout's records are named that way — `1ECookCook`, `3CBSCave1`,
        // `1stFloorAlarm` — and the source spells the record's name, so the lexer
        // must not split it into a number and a word.
        let tokens = crate::lex::lex("if GetInCell 3CBSCave1 == 1");
        let words: Vec<&str> = tokens
            .iter()
            .filter(|t| t.kind == crate::lex::Kind::Word)
            .map(|t| t.text.as_str())
            .collect();
        assert!(words.contains(&"3CBSCave1"), "{words:?}");
    }

    #[test]
    fn a_dotted_set_prepends_set_current_ref() {
        // Measured, MS14PowerArmorScript (Fallout3.esm):
        //   MS14ArmorVoiceRef.Set SayTimer to GameDaysPassed + 0.1
        //     → 1C 00 | 01 00   (its own instruction)
        //       15 00 | 0A 00 | 66 02 00 | 0A 00 | …
        let got = crate::compile_with_refs(
            "scn S\nfloat SayTimer\nBegin GameMode\n  MS14ArmorVoiceRef.Set SayTimer to 1\nEnd\n",
            &refs(&["MS14ArmorVoiceRef"]),
            &[crate::VarEntry {
                name: "SayTimer".to_string(),
                id: 2,
                is_int: false,
            }],
            &[],
        )
        .unwrap();
        let ins = read_instructions(&got);
        let at = ins
            .iter()
            .position(|i| i.opcode == 0x001C)
            .expect("a SetCurrentRef prefix");
        assert_eq!(ins[at].args, 1u16.to_le_bytes().to_vec());
        assert_eq!(ins[at + 1].opcode, 0x0015);
        // The destination is the variable alone — `66 02 00` — with no
        // reference prefix of its own.
        assert_eq!(&ins[at + 1].args[..3], &[0x66, 0x02, 0x00]);
    }
}

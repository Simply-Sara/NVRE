//! The expression encoder: a token run to its obscript value bytes.
//!
//! An expression's bytes are a tagged value:
//!
//! ```text
//! [tag:u8] [payload…]
//! ```
//!
//! and the tags are measured, not assumed. Over every `set` in
//! `FalloutNV.esm` the destination is one of
//!
//! - `0x73` `s` + a **1-based** `u16` index — a variable of *this* script
//! - `0x72` `r` + a **1-based** `u16` index — one of the record's references
//!
//! and the value being assigned is
//!
//! ```text
//! [len:u16] 0x20 [the literal's own bytes]
//! ```
//!
//! where `len` counts the `0x20` tag and everything after it. That is the
//! detail that is easy to get wrong: the length includes the tag, so a one
//! digit number is length 2, not 1. Measured over `73 01 00 02 00 20 31`
//! (assign 1 to variable 1), `73 01 00 03 00 20 34 30` (assign 40) and
//! `73 3D 00 05 00 20 34 35 30 30` (assign 4500 to variable 61).
//!
//! # Why the value's bytes are the source's own
//!
//! An integer literal is stored as its **ASCII digits**, not as a binary
//! integer: `1` is the single byte `0x31`, and `40` is `34 30`. This is
//! measurable and it matters, because a compiler that writes the binary
//! integer produces a body the game reads as a different number.

use crate::lex::{Kind, Token};

/// The tag that opens every literal's value bytes.
const LITERAL_TAG: u8 = 0x20;

/// A value the emitter can encode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// A `short`/`long`/`int` variable of this script, by its 1-based index.
    LocalVar(u16),
    /// A `float` or `ref` variable of this script, by its 1-based index.
    ///
    /// A separate tag from [`Value::LocalVar`], and the game's own bytes say
    /// which: `int PickedUp` assigns with `73 01 00` and `Ref rBox01` assigns
    /// with `66 01 00`. The distinction is the variable's declared type, not
    /// its index.
    FloatVar(u16),
    /// One of the record's references, by its 1-based index.
    Ref(u16),
    /// A global variable, by its 1-based index in the SCRO reference list.
    Global(u16),
    /// A literal: the bytes the source spelled it with, tag included.
    Literal(Vec<u8>),
    /// A command called with no arguments, used as a value.
    ///
    /// The `X` (0x58) token, the command's opcode, and a zero parameter
    /// length. This is how `set userref to getactionref` compiles.
    Command(u16),
}

/// Encode `value` as its tagged bytes.
pub fn encode(value: &Value) -> Vec<u8> {
    match value {
        // `s` + a 1-based index. The index is 1-based, which is measurable:
        // a script whose first declaration is `int PickedUp` and which sets
        // `PickedUp` writes index 1, not 0.
        Value::LocalVar(i) => {
            let mut out = vec![0x73];
            out.extend_from_slice(&i.to_le_bytes());
            out
        }
        // `f` + a 1-based index, for a `float` or a `ref`.
        Value::FloatVar(i) => {
            let mut out = vec![0x66];
            out.extend_from_slice(&i.to_le_bytes());
            out
        }
        // `r` + a 1-based index into the record's reference list.
        Value::Ref(i) => {
            let mut out = vec![0x72];
            out.extend_from_slice(&i.to_le_bytes());
            out
        }
        // `G` (0x47) + a 1-based index into the record's reference list for globals.
        Value::Global(i) => {
            let mut out = vec![0x47];
            out.extend_from_slice(&i.to_le_bytes());
            out
        }
        // The literal's own bytes, already tagged.
        Value::Literal(bytes) => bytes.clone(),
        // `X` + the opcode + a zero parameter length. The separator that
        // precedes every expression token is written by the expression, not by
        // the value.
        Value::Command(opcode) => {
            let mut out = vec![0x58];
            out.extend_from_slice(&opcode.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out
        }
    }
}

/// Encode the value part of an assignment: `[len:u16] 0x20 [bytes]`.
///
/// The length counts the tag and the literal's bytes together, which is what
/// the game writes — so a one-digit number is length 2.
///
/// The leading `0x20` is the **separator** that precedes every expression
/// token, not part of the value: an expression is a run of `0x20`-tagged
/// tokens with no per-token length, and `set PickedUp to 1` is
/// `02 00 20 31` — length 2 covering the separator and the digit.
pub fn encode_assigned(value: &Value) -> Vec<u8> {
    let mut tagged = vec![LITERAL_TAG];
    tagged.extend(encode(value));
    let mut out = Vec::with_capacity(2 + tagged.len());
    out.extend_from_slice(&(tagged.len() as u16).to_le_bytes());
    out.extend_from_slice(&tagged);
    out
}

/// Read `tokens` as a value, or say why it cannot be one.
///
/// A single number is a literal. A single word is a variable of this script,
/// which is resolved through the record's own variable table — the emitter
/// has the record's `SLSD` IDs, so the index is the table's, not a position
/// the source could have invented.
///
/// Anything else is refused: an expression this encoder does not understand is
/// a wrong encoding rather than a missing one, and the game runs a wrong
/// encoding.
pub fn read_value(tokens: &[Token], script: &crate::parse::Script) -> Result<Value, crate::Error> {
    // The value's own tokens, with the whitespace dropped: a value is the
    // words and numbers, and the spaces between them are not part of it.
    let words: Vec<&Token> = tokens
        .iter()
        .filter(|t| t.kind != Kind::Space && t.kind != Kind::Comment)
        .collect();

    if words.len() == 1 {
        let t = words[0];
        return match t.kind {
            Kind::Number => Ok(Value::Literal(literal_bytes(&t.text))),
            // A word on its own is a variable of this script. The index is
            // the record's SLSD ID for that name, and the tag follows the
            // variable's declared type — both measurable, and writing either
            // wrong is an assignment the game reads as a different value.
            Kind::Word => {
                if let Ok((index, is_int)) = resolve_var(&t.text, script) {
                    return Ok(if is_int {
                        Value::LocalVar(index)
                    } else {
                        Value::FloatVar(index)
                    });
                }
                if let Ok(idx) = resolve_ref_index(&t.text, script) {
                    return Ok(Value::Ref(idx));
                }
                if let Some(cmd) = crate::opcodes::command_called(&t.text) {
                    if !cmd.args.is_empty() {
                        return Err(crate::Error::Parse(format!(
                            "`{}` takes arguments, so it cannot be used as a value yet",
                            t.text
                        )));
                    }
                    return Ok(Value::Command(cmd.opcode));
                }
                Err(crate::Error::Parse(format!(
                    "`{}` is not a declared variable",
                    t.text
                )))
            }
            _ => Err(crate::Error::Parse(format!(
                "cannot encode `{}` as a value",
                t.text
            ))),
        };
    }

    Err(crate::Error::Parse(format!(
        "cannot encode `{}` as a value: only a number or a single variable name is understood so far",
        tokens
            .iter()
            .filter(|t| t.kind != Kind::Space)
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    )))
}

/// Encode an expression (e.g. for an `if` or `elseif` condition) into its
/// compiled bytecode bytes: a sequence of 0x20-separated tokens in postfix (RPN) order.
pub fn encode_expr(
    tokens: &[Token],
    script: &crate::parse::Script,
) -> Result<Vec<u8>, crate::Error> {
    // 1. Filter out whitespace and comments
    let significant: Vec<Token> = tokens
        .iter()
        .filter(|t| t.kind != Kind::Space && t.kind != Kind::Comment)
        .cloned()
        .collect();

    if significant.is_empty() {
        return Ok(Vec::new());
    }

    #[derive(Debug, Clone)]
    enum Item {
        Op(&'static str, u8), // symbol, precedence
        LParen,
    }

    fn op_info(tok: &Token) -> Option<(&'static str, u8)> {
        match tok.text.as_str() {
            // The GECK's compiler binds `||` TIGHTER than `&&` — the
            // opposite of every textbook. Measured on unparenthesized
            // corpus conditions: `A || B && C` compiles to the RPN
            // `A B || C &&` (VMS19NCRFightSCRIPT), and
            // `A && B && C && D || E` to `A B && C && D E || &&`
            // (VFSSilverRushGuardEscapeScript) — both are `||` grouping
            // first. Parenthesized conditions cannot discriminate and were
            // excluded.
            "||" => Some(("||", 2)),
            "&&" => Some(("&&", 1)),
            "==" => Some(("==", 3)),
            "!=" => Some(("!=", 3)),
            "<" => Some(("<", 4)),
            "<=" => Some(("<=", 4)),
            ">" => Some((">", 4)),
            ">=" => Some((">=", 4)),
            "+" => Some(("+", 5)),
            "-" => Some(("-", 5)),
            "*" => Some(("*", 6)),
            "/" => Some(("/", 6)),
            "%" => Some(("%", 6)),
            _ => None,
        }
    }

    /// Fold a sign that sits in an operand position into the number after it.
    ///
    /// `set X to -1` is a negation of the operand `1`, not a binary `-` with a
    /// missing left side: the game writes the value `20 31 20 7E` (the digits,
    /// then the postfix `~`), so the lexer's separate `-` token has to become
    /// part of the number before the shunting-yard sees it. A `-` after an
    /// operand is subtraction and is left alone — `iCount - 1` stays binary,
    /// which is `2D`, not `7E`.
    fn fold_unary_signs(tokens: &[Token]) -> Vec<Token> {
        let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
        let mut k = 0;
        while k < tokens.len() {
            let t = &tokens[k];
            let operand_position = match out.last() {
                None => true,
                Some(prev) => op_info(prev).is_some() || prev.text == "(" || prev.text == ",",
            };
            if operand_position
                && (t.text == "-" || t.text == "+")
                && k + 1 < tokens.len()
                && tokens[k + 1].kind == Kind::Number
            {
                let mut number = tokens[k + 1].clone();
                number.text = format!("{}{}", t.text, number.text);
                out.push(number);
                k += 2;
                continue;
            }
            // A sign in front of a *name* is a negation of that operand, and
            // the game writes it postfix: `set shoot to -shoot` compiles to
            // `20 73 02 00 20 7E` (the variable, then `~`), measured in
            // Fallout3.esm's MS01QuestScript. A `-` after an operand is
            // subtraction and stays `2D`.
            if operand_position
                && t.text == "-"
                && k + 1 < tokens.len()
                && matches!(tokens[k + 1].kind, Kind::Word)
            {
                out.push(tokens[k + 1].clone());
                let mut negate = tokens[k + 1].clone();
                negate.text = "~".to_string();
                negate.kind = Kind::Punct;
                out.push(negate);
                k += 2;
                continue;
            }
            out.push(t.clone());
            k += 1;
        }
        out
    }

    let filtered: Vec<Token> = fold_unary_signs(&significant);

    let mut output: Vec<Vec<u8>> = Vec::new();
    let mut op_stack: Vec<Item> = Vec::new();


    let mut i = 0;
    while i < filtered.len() {
        let t = &filtered[i];

        if t.text == "(" {
            op_stack.push(Item::LParen);
            i += 1;
            continue;
        }

        if t.text == ")" {
            while let Some(top) = op_stack.pop() {
                match top {
                    Item::LParen => break,
                    Item::Op(sym, _) => output.push(sym.as_bytes().to_vec()),
                }
            }
            i += 1;
            continue;
        }

        // The postfix negate operator, written by `fold_unary_signs`.
        if t.text == "~" {
            output.push(vec![b'~']);
            i += 1;
            continue;
        }

        if let Some((sym, prec)) = op_info(t) {
            while let Some(top) = op_stack.last() {
                match top {
                    Item::Op(_, top_prec) => {
                        if *top_prec >= prec {
                            if let Some(Item::Op(top_sym, _)) = op_stack.pop() {
                                output.push(top_sym.as_bytes().to_vec());
                            }
                        } else {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            op_stack.push(Item::Op(sym, prec));
            i += 1;
            continue;
        }

        // Check for dotted reference access or command call: e.g. ref.var or ref.Command
        if i + 2 < filtered.len() && filtered[i + 1].text == "." {
            let target = filtered[i].text.as_str();
            let member = filtered[i + 2].text.as_str();
            // Resolve ref index: declared ref variables, then SCRO refs.
            let ref_idx = resolve_ref_index(target, script).ok();

            if let Some(ridx) = ref_idx {
                // If member is a command on that ref:
                if let Some(cmd) = crate::opcodes::command_called(member) {
                    // Check if command takes arguments
                    let mut arg_tokens = Vec::new();
                    let mut j = i + 3;
                    while j < filtered.len() {
                        let next = &filtered[j];
                        // A sign at the start of an argument is part of the
                        // value, not a binary operator: `IsLimbGone -1`
                        // (Fallout3.esm RobcoRobotScript) passes -1 as the
                        // limb, and reading the `-` as subtraction gives a call
                        // with no arguments followed by a stray operator.
                        let leading_sign = arg_tokens.is_empty()
                            && (next.text == "-" || next.text == "+")
                            && filtered.get(j + 1).is_some_and(|n| n.kind == Kind::Number);
                        if (!leading_sign && op_info(next).is_some())
                            || next.text == ")"
                            || next.text == "("
                        {
                            break;
                        }
                        if leading_sign {
                            if let Some(n) = filtered.get(j + 1) {
                                let mut tok = n.clone();
                                tok.text = format!("{}{}", next.text, n.text);
                                arg_tokens.push(tok);
                                j += 2;
                                if arg_tokens.len() == cmd.args.len() {
                                    break;
                                }
                                continue;
                            }
                        }
                        arg_tokens.push(next.clone());
                        j += 1;
                        if arg_tokens.len() == cmd.args.len() {
                            break;
                        }
                    }

                    let mut arg_bytes = Vec::new();
                    let count = arg_tokens.len() as u16;
                    // One encoder for every argument position, the type's
                    // first: the statement form and both expression forms must
                    // not disagree about what a name in a given position is.
                    for (p_idx, atok) in arg_tokens.iter().enumerate() {
                        arg_bytes
                            .extend(crate::emit::encode_command_arg(atok, script, cmd, p_idx)?);
                    }

                    let mut b = vec![0x72];
                    b.extend_from_slice(&ridx.to_le_bytes());
                    b.push(0x58); // 'X'
                                  // The expression-form opcode, which for GetAV is not the
                                  // statement form's: measured, all 52 expression calls in
                                  // the corpus write 0x100E.
                    let expr_op = crate::opcodes::expression_opcode(member).ok_or_else(|| {
                        crate::Error::Parse(format!("`{member}` is not a known command"))
                    })?;
                    b.extend_from_slice(&expr_op.to_le_bytes());
                    if cmd.args.is_empty() && count == 0 {
                        b.extend_from_slice(&0u16.to_le_bytes());
                    } else {
                        let param_bytes = (2 + arg_bytes.len()) as u16;
                        b.extend_from_slice(&param_bytes.to_le_bytes());
                        b.extend_from_slice(&count.to_le_bytes());
                        b.extend(arg_bytes);
                    }
                    output.push(b);
                    i = j;
                    continue;
                } else {
                    // Not a command: a remote script variable,
                    // `Script.var`. Measured: `72 [refIdx: u16] [tag: u8]
                    // [varId: u16]`, where refIdx is the target's index in
                    // this record's reference space and varId is the
                    // variable's SLSD ID in the target script's own table —
                    // cross-record data the caller supplies.
                    let (tag, id) = resolve_remote_var(target, member, script)?;
                    let mut b = vec![0x72];
                    b.extend_from_slice(&ridx.to_le_bytes());
                    b.push(tag);
                    b.extend_from_slice(&id.to_le_bytes());
                    output.push(b);
                    i += 3;
                    continue;
                }
            }

            return Err(crate::Error::Parse(format!(
                "dotted references in expressions are not emitted yet: `{}`",
                t.text
            )));
        }

        // Function call with arguments inside expression (e.g. IsActionRef Player, GetDistance Player, etc.)
        if let Some(cmd) = crate::opcodes::command_called(&t.text) {
            if !cmd.args.is_empty() {
                // Consume command arguments from filtered tokens
                let mut arg_tokens = Vec::new();
                let mut j = i + 1;
                while j < filtered.len() {
                    let next = &filtered[j];
                    let leading_sign = arg_tokens.is_empty()
                        && (next.text == "-" || next.text == "+")
                        && filtered.get(j + 1).is_some_and(|n| n.kind == Kind::Number);
                    if (!leading_sign && op_info(next).is_some())
                        || next.text == ")"
                        || next.text == "("
                    {
                        break;
                    }
                    if leading_sign {
                        if let Some(n) = filtered.get(j + 1) {
                            let mut tok = n.clone();
                            tok.text = format!("{}{}", next.text, n.text);
                            arg_tokens.push(tok);
                            j += 2;
                            if arg_tokens.len() == cmd.args.len() {
                                break;
                            }
                            continue;
                        }
                    }
                    arg_tokens.push(next.clone());
                    j += 1;
                    if arg_tokens.len() == cmd.args.len() {
                        break;
                    }
                }

                // Encode arguments for function call: the same encoder the
                // statement form uses, so a name, a code and a float mean the
                // same thing in both.
                let mut arg_bytes = Vec::new();
                let count = arg_tokens.len() as u16;
                for (p_idx, atok) in arg_tokens.iter().enumerate() {
                    arg_bytes.extend(crate::emit::encode_command_arg(atok, script, cmd, p_idx)?);
                }

                let mut b = vec![0x58]; // 'X'
                                        // The expression-form opcode, which for GetAV is not the
                                        // statement form's: measured, all 52 expression calls in the
                                        // corpus write 0x100E where the statement form writes 0x11CD.
                let expr_op = crate::opcodes::expression_opcode(&t.text).ok_or_else(|| {
                    crate::Error::Parse(format!("`{}` is not a known command", t.text))
                })?;
                b.extend_from_slice(&expr_op.to_le_bytes());
                // paramBytes = 2 (for count) + arg_bytes.len()
                let param_bytes = (2 + arg_bytes.len()) as u16;
                b.extend_from_slice(&param_bytes.to_le_bytes());
                b.extend_from_slice(&count.to_le_bytes());
                b.extend(arg_bytes);

                output.push(b);
                i = j;
                continue;
            }
        }

        // Standalone reference or global variable in expression.
        // A declared variable comes first in every form — including a
        // declared `ref` variable, which is the `66 <SLSD id>` form here
        // and only becomes the `72`/`5A` reference-index form when it is
        // NOT a variable (a placed reference or SCRO entry). Measured:
        // `if (rUser == Player)` in NVEcvDoorBG01SCRIPT writes
        // `66 01 00` for the declared `ref rUser`, not `5A 01`.
        if let Ok((idx, is_int)) = resolve_var(&t.text, script) {
            let mut b = vec![var_tag(is_int)];
            b.extend_from_slice(&idx.to_le_bytes());
            output.push(b);
            i += 1;
            continue;
        }
        // A standalone SCRO name: the tag follows what the entry names —
        // a global variable (GLOB) writes `47` 'G', a placed reference
        // writes `5A` 'Z'. Measured corpus-wide: 360 of 365 `47` operands
        // name a GLOB record; the `5A` operands name REFR/ACHR/ACRE forms.
        // The entry's kind comes from the record, which the caller
        // supplies — a name the ref list does not carry is refused, not
        // guessed.
        if let Ok(idx) = resolve_ref_index(&t.text, script) {
            let kind = script
                .refs
                .iter()
                .find(|r| r.name.eq_ignore_ascii_case(&t.text))
                .map(|r| r.kind)
                .unwrap_or(crate::parse::RefKind::Reference);
            let tag = match kind {
                crate::parse::RefKind::Global => 0x47,
                crate::parse::RefKind::Reference => 0x5A,
            };
            let mut b = vec![tag];
            b.extend_from_slice(&idx.to_le_bytes());
            output.push(b);
            i += 1;
            continue;
        }

        // It is an operand: a literal, a declared variable, or a command value
        let val = read_value(std::slice::from_ref(t), script)?;
        if let Value::Literal(bytes) = &val {
            // A negative literal is its digits plus the postfix negate
            // operator `~` (0x7E) — measured: `== -5` writes
            // `20 35 20 7E` (the literal "5", then `~`), never a signed
            // ASCII literal, and `Set iSceneState to -1` writes the value
            // `20 31 20 7E`. The lexer folds the sign into the Number
            // token; the encoder splits it back out here.
            if bytes.first() == Some(&b'-') {
                // Postfix: the operand's digits first, then the negate
                // operator — `20 35 20 7E`, not the reverse.
                output.push(bytes[1..].to_vec());
                output.push(vec![b'~']);
            } else {
                output.push(bytes.clone());
            }
        } else {
            output.push(encode(&val));
        }
        i += 1;
    }

    while let Some(top) = op_stack.pop() {
        match top {
            Item::LParen => {
                return Err(crate::Error::Parse(
                    "unmatched `(` in expression".to_string(),
                ));
            }
            Item::Op(sym, _) => output.push(sym.as_bytes().to_vec()),
        }
    }

    // Now format into 0x20-prefixed sequence
    let mut result = Vec::new();
    for token_bytes in output {
        result.push(LITERAL_TAG); // 0x20
        result.extend(token_bytes);
    }

    Ok(result)
}

/// The tagged bytes for a numeric literal.
///
/// An integer is stored as its ASCII digits — `1` is the single byte `0x31`
/// and `40` is `34 30` — which is measurable over the corpus and is the
/// opposite of what a compiler would naturally write.
fn literal_bytes(text: &str) -> Vec<u8> {
    // The separator is written by `encode_assigned`, once per expression token,
    // so a literal carries only its own digits.
    text.as_bytes().to_vec()
}

/// Resolve a local variable's bytecode index and tag, by name.
///
/// The index is the variable's `SLSD` ID, taken from the record's own
/// variable table — measured over all 4,902 tagged variable references in
/// `FalloutNV.esm`'s `If`/`ElseIf` conditions: every index equals some
/// `SLSD`'s first word (the ID the GECK assigned), and only 58% equal the
/// variable's position in the table, because the GECK's IDs have gaps. The
/// ID cannot be derived from the source, so it is read from the record: a
/// name the table does not carry is a variable the record's writer has not
/// given an ID yet, and that is refused rather than guessed — a guessed ID
/// is an assignment to the wrong variable that the game silently runs.
pub fn resolve_var(name: &str, script: &crate::parse::Script) -> Result<(u16, bool), crate::Error> {
    if let Some(v) = script
        .vars
        .iter()
        .find(|v| v.name.eq_ignore_ascii_case(name))
    {
        return Ok((v.id, v.is_int));
    }
    // A declaration the record's table does not carry is a new variable. Its
    // ID is assigned here — one past every ID the record already uses, in
    // declaration order — because the bytecode needs one and the record's
    // writer persists the SLSD list the compiler used. A name that is
    // neither in the table nor a declaration is an error, not a guess.
    if let Some(at) = script
        .decls
        .iter()
        .position(|d| d.name.eq_ignore_ascii_case(name))
    {
        let mut next = script
            .vars
            .iter()
            .map(|v| v.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        // Earlier new declarations each took an ID; count them.
        for d in &script.decls[..at] {
            if !script
                .vars
                .iter()
                .any(|v| v.name.eq_ignore_ascii_case(&d.name))
            {
                next += 1;
            }
        }
        let is_int = script.decls[at].kind == crate::parse::DeclKind::Int;
        return Ok((next, is_int));
    }
    Err(crate::Error::Parse(format!(
        "`{name}` is not a declared variable"
    )))
}

/// The tag a variable of this script is written with: `s` for an int, `f`
/// for a float or a ref.
fn var_tag(is_int: bool) -> u8 {
    if is_int {
        0x73
    } else {
        0x66
    }
}

/// Resolve a remote script variable's tag and ID.
///
/// `SomeScript.varName` in an expression or a `set` destination writes
/// `72 <refIdx> <tag> <varId>`: the target's index in this record's
/// reference space, then the variable's own tag and `SLSD` ID **in the
/// target script's table**. Both halves are data the compiling record does
/// not carry — the caller supplies the remote tables — so a name that
/// resolves nowhere is refused rather than given the wrong ID, which the
/// game would run as a read of a different variable.
pub fn resolve_remote_var(
    target: &str,
    member: &str,
    script: &crate::parse::Script,
) -> Result<(u8, u16), crate::Error> {
    let remote = script
        .remotes
        .iter()
        .find(|r| r.name.eq_ignore_ascii_case(target))
        .ok_or_else(|| {
            crate::Error::Parse(format!(
                "`{target}` names a remote script whose variable table the compiler does not have"
            ))
        })?;
    if let Some(var) = remote
        .vars
        .iter()
        .find(|v| v.name.eq_ignore_ascii_case(member))
    {
        return Ok((if var.is_int { 0x73 } else { 0x66 }, var.id));
    }
    // A name the target's table does not carry is resolved in the compiling
    // script's own table instead — measured, `set Follower1 to
    // GomorrahCasinoEnterTriggerREF.Follower1` in GomorrahHotelDoorScript
    // writes `66 01 00 | 07 00 | 20 72 0E 00 66 01 00`: the ref is the
    // trigger's slot (0x0E) and the variable is `Follower1` at *this* script's
    // own SLSD id 1, because the trigger's script has no variable by that name
    // at all. The GECK apparently falls back the same way; without the
    // fallback the unit is refused rather than compiled.
    if let Ok((id, is_int)) = resolve_var(member, script) {
        return Ok((if is_int { 0x73 } else { 0x66 }, id));
    }
    Err(crate::Error::Parse(format!(
        "`{member}` is not a variable of remote script `{target}`"
    )))
}

/// Resolve a reference name to its 1-based index in the record's reference
/// list.
///
/// The list is data, not derived: the record writes its `SCRO` and `SCRV`
/// subrecords interleaved, in the order the GECK wrote them, and every
/// reference index the bytecode carries reads that one list, 1-based. See
/// [`crate::parse::RefEntry`] for the measurement — a declared-refs-first
/// model fits only the records whose `SCRV` entries happen to be written
/// before their `SCRO` entries.
pub fn resolve_ref_index(name: &str, script: &crate::parse::Script) -> Result<u16, crate::Error> {
    // The first slot a name occupies. A record may name one form twice, and
    // the game then picks inconsistently — MS16AmbushTriggerBSCRIPT
    // (Fallout3.esm) names `Player` at slots 2 and 6 and writes slot 2 for its
    // `BEGIN OnTriggerEnter PlayerREF` and slot 6 for a `GetDistance player`
    // operand in the same script. Nothing in the reference list distinguishes
    // the two, so the first is taken (which is right for every unit in
    // FalloutNV.esm) and that one Fallout 3 unit is left differing.
    resolve_ref_index_where(name, script, false)
}

/// The same lookup, taking the *first* slot a name occupies.
///
/// A record can name one form twice — MS16AmbushTriggerBSCRIPT's list has
/// `Player` at slots 2 and 6 — and the game is not consistent about which it
/// writes: measured in that unit, the `Begin OnTriggerEnter PlayerREF`
/// parameter is slot 2 (the first) while the `GetDistance Player` operand in
/// the same script is slot 6 (the last). So the position asks for its own rule:
/// a `Begin` parameter takes the first, everything else the last.
pub fn resolve_ref_index_first(
    name: &str,
    script: &crate::parse::Script,
) -> Result<u16, crate::Error> {
    resolve_ref_index_where(name, script, false)
}

fn resolve_ref_index_where(
    name: &str,
    script: &crate::parse::Script,
    prefer_last: bool,
) -> Result<u16, crate::Error> {
    let matches: Vec<usize> = script
        .refs
        .iter()
        .enumerate()
        .filter(|(_, r)| r.name.eq_ignore_ascii_case(name))
        .map(|(i, _)| i)
        .collect();
    if let Some(at) = if prefer_last {
        matches.last().copied()
    } else {
        matches.first().copied()
    } {
        return Ok(at as u16 + 1);
    }
    // `PlayerRef` is the source's other spelling for the record's `Player`
    // entry, and the game resolves it to that entry's own slot rather than to
    // FormID 0x14: measured, `if ( BoxingGloveTarget != PlayerRef )` in
    // BoxingGlovesFatigueOnHitScript writes `20 5A 02 00`, where slot 2 is the
    // `SCRO` entry named `Player`. Measured in 9 units, always the `Player`
    // entry's index.
    if name.eq_ignore_ascii_case("PlayerRef") {
        // The other spelling of the record's `Player` entry, and the same
        // first/last question: as an operand the game takes the later entry
        // (VES01VictorGSMyBodyguardScript writes `20 5A 07 00` where slot 7 is
        // the second `Player`), and as a `Begin` parameter the earlier one.
        let players: Vec<usize> = script
            .refs
            .iter()
            .enumerate()
            .filter(|(_, r)| r.name.eq_ignore_ascii_case("Player"))
            .map(|(i, _)| i)
            .collect();
        // The alias takes the *last* `Player` slot — measured in
        // VES01VictorGSMyBodyguardScript (`20 5A 07 00`, slot 7 of two) and
        // VHDKimballSpeechSCRIPT. It is the one name where the later entry is
        // the right one, which is why it has a rule of its own.
        let prefer_last = true;
        if let Some(at) = if prefer_last {
            players.last().copied()
        } else {
            players.first().copied()
        } {
            return Ok(at as u16 + 1);
        }
    }
    // No supplied list carries the name. The one case that still resolves
    // is a declared `ref` variable when the caller supplied no list at all
    // — its position among the declarations, which is what the record-order
    // list holds when the record writes its SCRV entries before any SCRO
    // (the common shape, and the only shape a caller with no list can
    // know). A placed reference name with no list is refused: its slot is
    // data the caller has to supply.
    if script.refs.is_empty() {
        let mut ref_var_idx = 0u16;
        for d in &script.decls {
            if d.kind == crate::parse::DeclKind::Ref {
                ref_var_idx += 1;
                if d.name.eq_ignore_ascii_case(name) {
                    return Ok(ref_var_idx);
                }
            }
        }
    }
    Err(crate::Error::Parse(format!(
        "`{name}` names a reference, which the emitter cannot resolve yet"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex::lex;

    fn value_of(source: &str, script: &crate::parse::Script) -> Result<Value, crate::Error> {
        let tokens = lex(source);
        read_value(&tokens, script)
    }

    fn script_with(
        vars: &[(&str, u16, bool)],
        decls: &[(&str, crate::parse::DeclKind)],
    ) -> crate::parse::Script {
        crate::parse::Script {
            decls: decls
                .iter()
                .map(|(name, kind)| crate::parse::Decl {
                    name: (*name).to_string(),
                    kind: *kind,
                    line: 0,
                })
                .collect(),
            vars: vars
                .iter()
                .map(|(name, id, is_int)| crate::parse::VarEntry {
                    name: (*name).to_string(),
                    id: *id,
                    is_int: *is_int,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_variables_index_is_the_records_slsd_id() {
        // Measured over all 4,902 tagged variable references in
        // FalloutNV.esm's If/ElseIf conditions: the index is the SLSD ID,
        // not the position in the table — the GECK's IDs have gaps, so a
        // variable can sit at position 30 and carry ID 13.
        let s = script_with(
            &[("bNoSquad", 13, true), ("iSquadState", 19, true)],
            &[("bNoSquad", crate::parse::DeclKind::Int)],
        );
        assert_eq!(value_of("bNoSquad", &s).unwrap(), Value::LocalVar(13));
        assert_eq!(value_of("iSquadState", &s).unwrap(), Value::LocalVar(19));
        // And the bytes carry the ID, not the position.
        assert_eq!(encode(&Value::LocalVar(13)), vec![0x73, 0x0D, 0x00]);
    }

    #[test]
    fn a_new_variable_gets_an_id_past_the_records_own() {
        // A declaration the record's table does not carry: the compiler
        // assigns the next ID past every ID the record already uses, because
        // the bytecode needs one and the record's writer persists the SLSD
        // list the compiler used.
        let s = script_with(
            &[("Existing", 13, true)],
            &[("NewVar", crate::parse::DeclKind::Int)],
        );
        assert_eq!(value_of("NewVar", &s).unwrap(), Value::LocalVar(14));
        // And two new declarations get consecutive IDs.
        let s = script_with(
            &[],
            &[
                ("First", crate::parse::DeclKind::Int),
                ("Second", crate::parse::DeclKind::Int),
            ],
        );
        assert_eq!(value_of("First", &s).unwrap(), Value::LocalVar(1));
        assert_eq!(value_of("Second", &s).unwrap(), Value::LocalVar(2));
    }

    #[test]
    fn a_variable_the_records_table_does_not_carry_is_refused() {
        // A name that is neither in the record's table nor a declaration is
        // an error rather than a guessed index.
        let err = value_of("NoSuchVar", &script_with(&[], &[])).unwrap_err();
        assert!(
            err.to_string().contains("NoSuchVar"),
            "the error names it: {err}"
        );
    }

    #[test]
    fn a_variables_index_is_case_insensitive() {
        // The game's own compiler treats `set pickedup` and `set PickedUp` as
        // the same variable, and so does this.
        let s = script_with(&[("PickedUp", 1, true)], &[]);
        assert_eq!(value_of("pickedup", &s).unwrap(), Value::LocalVar(1));
    }

    #[test]
    fn a_ref_variable_is_the_f_tag_and_an_int_is_the_s_tag() {
        // Measured, and the two tags are the game's own: `int PickedUp` assigns
        // with `73 01 00` and `Ref rBox01` assigns with `66 01 00`. The tag
        // follows the variable's declared type, not its index — writing the
        // wrong one is an assignment to a variable of the wrong type.
        let s = script_with(
            &[("PickedUp", 1, true), ("rBox01", 2, false)],
            &[
                ("PickedUp", crate::parse::DeclKind::Int),
                ("rBox01", crate::parse::DeclKind::Ref),
            ],
        );
        assert_eq!(value_of("PickedUp", &s).unwrap(), Value::LocalVar(1));
        assert_eq!(value_of("rBox01", &s).unwrap(), Value::FloatVar(2));
        // And their bytes are the `s` and `f` tags respectively.
        assert_eq!(encode(&Value::LocalVar(1)), vec![0x73, 0x01, 0x00]);
        assert_eq!(encode(&Value::FloatVar(2)), vec![0x66, 0x02, 0x00]);
    }

    #[test]
    fn an_undeclared_name_is_an_error_not_a_wrong_index() {
        // An index that would assign to the wrong variable is worse than no
        // assignment at all.
        let err = value_of("NoSuchVar", &script_with(&[], &[])).unwrap_err();
        assert!(err.to_string().contains("NoSuchVar"), "{err}");
    }

    #[test]
    fn an_integer_literal_is_its_own_ascii_digits() {
        // Measured: `set PickedUp to 1` writes `02 00 20 31`, where 0x31 is
        // the ASCII `1` — not the binary integer 1.
        assert_eq!(
            value_of("1", &script_with(&[], &[])).unwrap(),
            Value::Literal(vec![0x31])
        );
        // And `to 40` writes `03 00 20 34 30`.
        assert_eq!(
            value_of("40", &script_with(&[], &[])).unwrap(),
            Value::Literal(vec![0x34, 0x30])
        );
    }

    #[test]
    fn the_assigned_length_counts_the_tag() {
        // The length is 2 for a one-digit number: the tag plus the digit.
        // Getting this wrong by one shifts every byte after it.
        assert_eq!(
            encode_assigned(&Value::Literal(vec![0x31])),
            vec![0x02, 0x00, 0x20, 0x31]
        );
        assert_eq!(
            encode_assigned(&Value::Literal(vec![0x34, 0x30])),
            vec![0x03, 0x00, 0x20, 0x34, 0x30]
        );
    }

    #[test]
    fn a_variable_destination_is_the_s_tag_and_its_index() {
        assert_eq!(encode(&Value::LocalVar(1)), vec![0x73, 0x01, 0x00]);
        // Variable 61, from `73 3D 00` in the corpus.
        assert_eq!(encode(&Value::LocalVar(61)), vec![0x73, 0x3D, 0x00]);
    }

    #[test]
    fn a_reference_is_the_r_tag_and_its_index() {
        // `set vdialoguegomorrah.iVMS21Fail` writes `72 01 00` for the first
        // reference, so the tag is `r` and the index is 1-based.
        assert_eq!(encode(&Value::Ref(1)), vec![0x72, 0x01, 0x00]);
    }

    #[test]
    fn an_expression_this_encoder_does_not_understand_is_refused() {
        // An arithmetic expression is a wrong encoding rather than a missing
        // one, and the game runs a wrong encoding.
        let err = value_of("1 + 2", &script_with(&[], &[])).unwrap_err();
        assert!(err.to_string().contains("1 + 2"), "{err}");
    }
}

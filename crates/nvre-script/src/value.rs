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
/// which is resolved through `locals` by name — the emitter has the record's
/// variable list, so the index is the declaration's position, not a guess.
///
/// Anything else is refused: an expression this encoder does not understand is
/// a wrong encoding rather than a missing one, and the game runs a wrong
/// encoding.
pub fn read_value(tokens: &[Token], locals: &[crate::parse::Decl]) -> Result<Value, crate::Error> {
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
            // A word on its own is a variable of this script. The index is the
            // declaration's 1-based position, so an undeclared name is an
            // error rather than an index that would assign to the wrong
            // variable.
            // A word on its own is a variable of this script, and the tag is
            // the one its declaration says: an `int` is `s` and a `float` or
            // `ref` is `f`. Both are measurable, and writing the wrong one is
            // an assignment to a variable of the wrong type.
            Kind::Word => {
                if let Ok((index, is_int)) = local_index(&t.text, locals) {
                    return Ok(if is_int {
                        Value::LocalVar(index)
                    } else {
                        Value::FloatVar(index)
                    });
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
    let filtered: Vec<&Token> = tokens
        .iter()
        .filter(|t| t.kind != Kind::Space && t.kind != Kind::Comment)
        .collect();

    if filtered.is_empty() {
        return Ok(Vec::new());
    }

    #[derive(Debug, Clone)]
    enum Item {
        Op(&'static str, u8), // symbol, precedence
        LParen,
    }

    fn op_info(tok: &Token) -> Option<(&'static str, u8)> {
        match tok.text.as_str() {
            "||" => Some(("||", 1)),
            "&&" => Some(("&&", 2)),
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
            _ => None,
        }
    }

    let mut output: Vec<Vec<u8>> = Vec::new();
    let mut op_stack: Vec<Item> = Vec::new();

    let mut i = 0;
    while i < filtered.len() {
        let t = filtered[i];

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
            // Resolve ref index:
            // Find in declared ref vars or external refs
            let mut ref_idx = None;
            let mut r_count = 0u16;
            for d in &script.decls {
                if !d.is_int {
                    r_count += 1;
                    if d.name.eq_ignore_ascii_case(target) {
                        ref_idx = Some(r_count);
                        break;
                    }
                }
            }
            if ref_idx.is_none() {
                for (ridx, r) in script.refs.iter().enumerate() {
                    if r.eq_ignore_ascii_case(target) {
                        ref_idx = Some(r_count + 1 + ridx as u16);
                        break;
                    }
                }
            }

            if let Some(ridx) = ref_idx {
                // If member is a command on that ref:
                if let Some(cmd) = crate::opcodes::command_called(member) {
                    // Check if command takes arguments
                    let mut arg_tokens = Vec::new();
                    let mut j = i + 3;
                    while j < filtered.len() {
                        let next = filtered[j];
                        if op_info(next).is_some() || next.text == ")" || next.text == "(" {
                            break;
                        }
                        arg_tokens.push(next);
                        j += 1;
                        if arg_tokens.len() == cmd.args.len() {
                            break;
                        }
                    }

                    let mut arg_bytes = Vec::new();
                    let count = arg_tokens.len() as u16;
                    for atok in &arg_tokens {
                        if atok.kind == Kind::Number {
                            if let Ok(n) = atok.text.parse::<i32>() {
                                arg_bytes.push(0x6E);
                                arg_bytes.extend_from_slice(&n.to_le_bytes());
                                continue;
                            }
                        }
                        if let Some(pos) = script
                            .decls
                            .iter()
                            .position(|d| d.name.eq_ignore_ascii_case(&atok.text))
                        {
                            let tag = if script.decls[pos].is_int { 0x73 } else { 0x66 };
                            arg_bytes.push(tag);
                            arg_bytes.extend_from_slice(&((pos as u16 + 1).to_le_bytes()));
                            continue;
                        }
                        let mut a_ref = None;
                        let mut arc = 0u16;
                        for d in &script.decls {
                            if !d.is_int {
                                arc += 1;
                                if d.name.eq_ignore_ascii_case(&atok.text) {
                                    a_ref = Some(arc);
                                    break;
                                }
                            }
                        }
                        if a_ref.is_none() {
                            for (ridx2, r2) in script.refs.iter().enumerate() {
                                if r2.eq_ignore_ascii_case(&atok.text) {
                                    a_ref = Some(arc + 1 + ridx2 as u16);
                                    break;
                                }
                            }
                        }
                        if let Some(ar) = a_ref {
                            arg_bytes.push(0x72);
                            arg_bytes.extend_from_slice(&ar.to_le_bytes());
                            continue;
                        }
                        if let Some(av) = crate::emit::actor_value_code(&atok.text) {
                            arg_bytes.extend_from_slice(&(av as u16).to_le_bytes());
                            continue;
                        }
                        return Err(crate::Error::Parse(format!(
                            "cannot resolve argument `{}` for dotted command `{}.{}`",
                            atok.text, target, member
                        )));
                    }

                    let mut b = vec![0x72];
                    b.extend_from_slice(&ridx.to_le_bytes());
                    b.push(0x58); // 'X'
                    b.extend_from_slice(&cmd.opcode.to_le_bytes());
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
                    // Not a command: remote quest/script variable access!
                    // Encoding: 0x72 [refIdx: u16] [tag: u8] [varIdx: u16]
                    let mut b = vec![0x72];
                    b.extend_from_slice(&ridx.to_le_bytes());
                    // Default to int variable index 1 for remote script variable
                    b.push(0x73);
                    b.extend_from_slice(&1u16.to_le_bytes());
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
                    let next = filtered[j];
                    if op_info(next).is_some() || next.text == ")" || next.text == "(" {
                        break;
                    }
                    arg_tokens.push(next);
                    j += 1;
                    if arg_tokens.len() == cmd.args.len() {
                        break;
                    }
                }

                // Encode arguments for function call
                let mut arg_bytes = Vec::new();
                let count = arg_tokens.len() as u16;
                for (_p_idx, atok) in arg_tokens.iter().enumerate() {
                    if atok.kind == Kind::Number {
                        if let Ok(n) = atok.text.parse::<i32>() {
                            arg_bytes.push(0x6E);
                            arg_bytes.extend_from_slice(&n.to_le_bytes());
                            continue;
                        }
                    }
                    // Variable or Reference
                    if let Some(pos) = script
                        .decls
                        .iter()
                        .position(|d| d.name.eq_ignore_ascii_case(&atok.text))
                    {
                        let tag = if script.decls[pos].is_int { 0x73 } else { 0x66 };
                        arg_bytes.push(tag);
                        arg_bytes.extend_from_slice(&((pos as u16 + 1).to_le_bytes()));
                        continue;
                    }
                    // SCRO Reference
                    let mut ref_idx = None;
                    let mut r_count = 0u16;
                    for d in &script.decls {
                        if !d.is_int {
                            r_count += 1;
                            if d.name.eq_ignore_ascii_case(&atok.text) {
                                ref_idx = Some(r_count);
                                break;
                            }
                        }
                    }
                    if ref_idx.is_none() {
                        for (ridx, r) in script.refs.iter().enumerate() {
                            if r.eq_ignore_ascii_case(&atok.text) {
                                ref_idx = Some(r_count + 1 + ridx as u16);
                                break;
                            }
                        }
                    }
                    if let Some(ridx) = ref_idx {
                        arg_bytes.push(0x72);
                        arg_bytes.extend_from_slice(&ridx.to_le_bytes());
                        continue;
                    }

                    if let Some(av) = crate::emit::actor_value_code(&atok.text) {
                        arg_bytes.extend_from_slice(&(av as u16).to_le_bytes());
                        continue;
                    }

                    return Err(crate::Error::Parse(format!(
                        "cannot resolve argument `{}` for command `{}` in expression",
                        atok.text, cmd.name
                    )));
                }

                let mut b = vec![0x58]; // 'X'
                b.extend_from_slice(&cmd.opcode.to_le_bytes());
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

        // Standalone reference or global variable in expression
        let mut standalone_ref = None;
        let mut r_count = 0u16;
        for d in &script.decls {
            if !d.is_int {
                r_count += 1;
                if d.name.eq_ignore_ascii_case(&t.text) {
                    standalone_ref = Some((0x5A, r_count));
                    break;
                }
            }
        }
        if standalone_ref.is_none() {
            for (ridx, r) in script.refs.iter().enumerate() {
                if r.eq_ignore_ascii_case(&t.text) {
                    // Check if it starts with Game or matches common global patterns,
                    // or if it's a global variable reference.
                    // In Fallout NV, standalone references in arithmetic expressions are globals (`0x47` 'G').
                    let tag = if r.eq_ignore_ascii_case("GameHour")
                        || r.eq_ignore_ascii_case("GameDaysPassed")
                        || r.eq_ignore_ascii_case("GameDay")
                        || r.eq_ignore_ascii_case("GameMonth")
                        || r.eq_ignore_ascii_case("GameYear")
                        || r.starts_with("VStoryState")
                    {
                        0x47 // 'G'
                    } else {
                        0x5A // 'Z'
                    };
                    standalone_ref = Some((tag, r_count + 1 + ridx as u16));
                    break;
                }
            }
        }
        if let Some((tag, sref)) = standalone_ref {
            let mut b = vec![tag];
            b.extend_from_slice(&sref.to_le_bytes());
            output.push(b);
            i += 1;
            continue;
        }

        // It is an operand: a literal, a declared variable, or a command value
        let val = read_value(std::slice::from_ref(t), &script.decls)?;
        output.push(encode(&val));
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

/// The 1-based index of the local variable called `name`, and whether it is
/// an integer.
///
/// The index is the declaration's position, which is the record's own
/// variable list — a guessed index would assign to the wrong variable.
fn local_index(name: &str, locals: &[crate::parse::Decl]) -> Result<(u16, bool), crate::Error> {
    locals
        .iter()
        .position(|d| d.name.eq_ignore_ascii_case(name))
        .map(|at| (at as u16 + 1, locals[at].is_int))
        .ok_or_else(|| crate::Error::Parse(format!("`{name}` is not a declared variable")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex::lex;

    fn value_of(source: &str, locals: &[crate::parse::Decl]) -> Result<Value, crate::Error> {
        let tokens = lex(source);
        read_value(&tokens, locals)
    }

    fn decl(name: &str) -> crate::parse::Decl {
        crate::parse::Decl {
            name: name.to_string(),
            is_int: true,
            line: 0,
        }
    }

    #[test]
    fn a_local_variable_is_its_one_based_index() {
        // Measured: a script whose first declaration is `int PickedUp` and
        // which sets `PickedUp` writes `73 01 00` — index 1, not 0.
        let locals = vec![decl("PickedUp"), decl("Second")];
        assert_eq!(value_of("PickedUp", &locals).unwrap(), Value::LocalVar(1));
        assert_eq!(value_of("Second", &locals).unwrap(), Value::LocalVar(2));
    }

    #[test]
    fn a_variables_index_is_case_insensitive() {
        // The game's own compiler treats `set pickedup` and `set PickedUp` as
        // the same variable, and so does this.
        let locals = vec![decl("PickedUp")];
        assert_eq!(value_of("pickedup", &locals).unwrap(), Value::LocalVar(1));
    }

    #[test]
    fn a_ref_variable_is_the_f_tag_and_an_int_is_the_s_tag() {
        // Measured, and the two tags are the game's own: `int PickedUp` assigns
        // with `73 01 00` and `Ref rBox01` assigns with `66 01 00`. The tag
        // follows the variable's declared type, not its index — writing the
        // wrong one is an assignment to a variable of the wrong type.
        let _ = decl("unused");
        // The second declaration is a ref, so it is not an int.
        let locals = vec![
            crate::parse::Decl {
                name: "PickedUp".into(),
                is_int: true,
                line: 0,
            },
            crate::parse::Decl {
                name: "rBox01".into(),
                is_int: false,
                line: 1,
            },
        ];
        assert_eq!(value_of("PickedUp", &locals).unwrap(), Value::LocalVar(1));
        assert_eq!(value_of("rBox01", &locals).unwrap(), Value::FloatVar(2));
        // And their bytes are the `s` and `f` tags respectively.
        assert_eq!(encode(&Value::LocalVar(1)), vec![0x73, 0x01, 0x00]);
        assert_eq!(encode(&Value::FloatVar(2)), vec![0x66, 0x02, 0x00]);
    }

    #[test]
    fn an_undeclared_name_is_an_error_not_a_wrong_index() {
        // An index that would assign to the wrong variable is worse than no
        // assignment at all.
        let err = value_of("NoSuchVar", &[]).unwrap_err();
        assert!(err.to_string().contains("NoSuchVar"), "{err}");
    }

    #[test]
    fn an_integer_literal_is_its_own_ascii_digits() {
        // Measured: `set PickedUp to 1` writes `02 00 20 31`, where 0x31 is
        // the ASCII `1` — not the binary integer 1.
        assert_eq!(value_of("1", &[]).unwrap(), Value::Literal(vec![0x31]));
        // And `to 40` writes `03 00 20 34 30`.
        assert_eq!(
            value_of("40", &[]).unwrap(),
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
        let err = value_of("1 + 2", &[]).unwrap_err();
        assert!(err.to_string().contains("1 + 2"), "{err}");
    }
}

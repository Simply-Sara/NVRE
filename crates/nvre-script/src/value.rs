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
    /// A literal: the bytes the source spelled it with, tag included.
    Literal(Vec<u8>),
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
        // The literal's own bytes, already tagged.
        Value::Literal(bytes) => bytes.clone(),
    }
}

/// Encode the value part of an assignment: `[len:u16] 0x20 [bytes]`.
///
/// The length counts the tag and the literal's bytes together, which is what
/// the game writes — so a one-digit number is length 2.
pub fn encode_assigned(value: &Value) -> Vec<u8> {
    let tagged = encode(value);
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
                let (index, is_int) = local_index(&t.text, locals)?;
                if is_int {
                    Ok(Value::LocalVar(index))
                } else {
                    Ok(Value::FloatVar(index))
                }
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

/// The tagged bytes for a numeric literal.
///
/// An integer is stored as its ASCII digits — `1` is the single byte `0x31`
/// and `40` is `34 30` — which is measurable over the corpus and is the
/// opposite of what a compiler would naturally write.
fn literal_bytes(text: &str) -> Vec<u8> {
    let mut out = vec![LITERAL_TAG];
    out.extend_from_slice(text.as_bytes());
    out
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
        assert_eq!(
            value_of("1", &[]).unwrap(),
            Value::Literal(vec![0x20, 0x31])
        );
        // And `to 40` writes `03 00 20 34 30`.
        assert_eq!(
            value_of("40", &[]).unwrap(),
            Value::Literal(vec![0x20, 0x34, 0x30])
        );
    }

    #[test]
    fn the_assigned_length_counts_the_tag() {
        // The length is 2 for a one-digit number: the tag plus the digit.
        // Getting this wrong by one shifts every byte after it.
        assert_eq!(
            encode_assigned(&Value::Literal(vec![0x20, 0x31])),
            vec![0x02, 0x00, 0x20, 0x31]
        );
        assert_eq!(
            encode_assigned(&Value::Literal(vec![0x20, 0x34, 0x30])),
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

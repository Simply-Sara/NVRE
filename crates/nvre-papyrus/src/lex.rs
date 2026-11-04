//! Papyrus source into tokens.
//!
//! # The lexical rules that matter
//!
//! Papyrus is case-insensitive for keywords and identifiers, uses `;` for a
//! comment to end of line, and allows `{…}` doc comments in the positions where
//! the CK expects them (after a property declaration, before a function).
//!
//! - **A comment is not a token.** It is dropped here rather than skipped
//!   later, so the parser never has to know where one may appear.
//! - **`{` opens a doc comment only where a doc comment is legal**, which is
//!   the parser's business. The lexer emits it as a `DocComment` token and the
//!   parser decides whether the position accepts one — a `{` in an expression
//!   is not Papyrus and is reported rather than silently eating the rest of the
//!   file.
//! - **A number is not signed.** `-5` lexes as `Minus` then `Int(5)`; the
//!   compiler's own output shows unary minus is a real operation on the value,
//!   not part of the literal, so the lexer must not fold it.
//! - **A newline is not a token.** Papyrus is not newline-sensitive; a
//!   statement ends where the grammar says it does.
//!
//! # The lexer never fails
//!
//! Following `nvre-script`'s rule: an unrecognised character becomes an
//! [`Kind::Unknown`] token so an editor can highlight incomplete code rather
//! than choke on it mid-keystroke. The parser is where an unexpected token is
//! an error.

use std::fmt;

/// What a token is.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// An identifier or keyword. Keywords are recognised by the parser from
    /// this, lowercased, so `EndIf`, `endif` and `Endif` are one thing.
    Word(String),
    /// An integer literal.
    Int(i32),
    /// A float literal.
    Float(f32),
    /// A string literal, with its escapes resolved.
    Str(String),
    /// A `{…}` doc comment, with its braces and surrounding whitespace
    /// trimmed.
    DocComment(String),
    /// `(`.
    LParen,
    /// `)`.
    RParen,
    /// `[`.
    LBracket,
    /// `]`.
    RBracket,
    /// `,`.
    Comma,
    /// `.`.
    Dot,
    /// `=`.
    Assign,
    /// `:=` — a named argument (`f(x := 1)`) or a parameter default.
    ColonAssign,
    /// `:`.
    Colon,
    /// `==`.
    EqEq,
    /// `!=`.
    NotEq,
    /// `<`.
    Lt,
    /// `<=`.
    LtEq,
    /// `>`.
    Gt,
    /// `>=`.
    GtEq,
    /// `+`.
    Plus,
    /// `-`.
    Minus,
    /// `*`.
    Star,
    /// `/`.
    Slash,
    /// `%`.
    Percent,
    /// `!`.
    Bang,
    /// `&&`.
    AndAnd,
    /// `||`.
    OrOr,
    /// The end of a statement.
    ///
    /// Papyrus terminates a statement at a newline. Emitting the newline as a
    /// token is what stops `(a as B).C` on one line and `(a as B).C = true`
    /// on the next from being read as one call — the statement on the first
    /// line ends with `.C`, and the `(` that follows starts a *new* statement
    /// rather than an argument list.
    ///
    /// A newline inside parentheses or brackets is not emitted: a call whose
    /// arguments wrap across lines is one statement, and the depth counter here
    /// is what tells them apart.
    Newline,
    /// A character Papyrus does not use.
    Unknown(char),
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Word(w) => write!(f, "{w}"),
            Kind::Int(i) => write!(f, "{i}"),
            Kind::Float(x) => write!(f, "{x}"),
            Kind::Str(s) => write!(f, "{s:?}"),
            Kind::DocComment(_) => write!(f, "{{…}}"),
            Kind::LParen => write!(f, "("),
            Kind::RParen => write!(f, ")"),
            Kind::LBracket => write!(f, "["),
            Kind::RBracket => write!(f, "]"),
            Kind::Comma => write!(f, ","),
            Kind::Dot => write!(f, "."),
            Kind::Assign => write!(f, "="),
            Kind::ColonAssign => write!(f, ":="),
            Kind::Colon => write!(f, ":"),
            Kind::EqEq => write!(f, "=="),
            Kind::NotEq => write!(f, "!="),
            Kind::Lt => write!(f, "<"),
            Kind::LtEq => write!(f, "<="),
            Kind::Gt => write!(f, ">"),
            Kind::GtEq => write!(f, ">="),
            Kind::Plus => write!(f, "+"),
            Kind::Minus => write!(f, "-"),
            Kind::Star => write!(f, "*"),
            Kind::Slash => write!(f, "/"),
            Kind::Percent => write!(f, "%"),
            Kind::Bang => write!(f, "!"),
            Kind::AndAnd => write!(f, "&&"),
            Kind::OrOr => write!(f, "||"),
            Kind::Unknown(c) => write!(f, "{c:?}"),
            Kind::Newline => write!(f, "end of statement"),
        }
    }
}

/// A token and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// What it is.
    pub kind: Kind,
    /// The 1-based source line it starts on. The debug block records a line
    /// per instruction, so every token carries the line that produced it.
    pub line: u16,
    /// The 1-based **column** it starts on, counted in characters.
    ///
    /// An editor needs this and a line number alone is not enough: without a
    /// column there is nothing to underline, and a diagnostic can only point at
    /// the whole line. It is computed from the offset of the last newline rather
    /// than maintained incrementally, so it cannot drift from the position the
    /// token was actually read at.
    pub col: u16,
}

impl Token {
    /// The token's text, lowercased, when it is a word.
    pub fn word(&self) -> Option<String> {
        match &self.kind {
            Kind::Word(w) => Some(w.to_ascii_lowercase()),
            _ => None,
        }
    }
}

/// Tokenise `src`.
pub fn lex(src: &str) -> Vec<Token> {
    let bytes: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut line: u16 = 1;
    // Index just past the last newline. A token's column is its start offset
    // minus this, so the column is derived from the position rather than kept in
    // step by hand at every `i += 1`.
    let mut last_nl = 0usize;
    // How deep inside `(`/`[` the reader is. A newline at depth 0 ends a
    // statement; one inside a bracket does not.
    let mut depth: i32 = 0;

    while i < bytes.len() {
        let c = bytes[i];
        // Where this token starts, for its column.
        let tok_start = i;
        match c {
            '\n' => {
                if depth == 0 && !out.is_empty() {
                    out.push(Token {
                        kind: Kind::Newline,
                        line,
                        col: (tok_start - last_nl + 1) as u16,
                    });
                }
                line = line.saturating_add(1);
                i += 1;
                last_nl = i;
            }
            c if c.is_whitespace() => i += 1,
            ';' if bytes.get(i + 1) == Some(&'/') => {
                // `;/ … /;` is a block comment, and it may span lines and
                // contain anything. Measured in `ArenaCombatantScript.psc`,
                // whose `Event OnInit` body is entirely commented out this way
                // — treating `;/` as a line comment leaves the body's last line
                // (`endif /;`) as code, and the `)` that follows reads as a
                // syntax error several statements later.
                i += 2;
                while i < bytes.len() {
                    if bytes[i] == '/' && bytes.get(i + 1) == Some(&';') {
                        i += 2;
                        break;
                    }
                    if bytes[i] == '\n' {
                        line = line.saturating_add(1);
                    }
                    i += 1;
                }
            }
            ';' => {
                // A line comment runs to the end of the line.
                while i < bytes.len() && bytes[i] != '\n' {
                    i += 1;
                }
            }
            '\\' => {
                // A backslash at the end of a line continues the statement on
                // the next one: `Event OnHit(ObjectReference akTarget, \` then
                // the rest of the parameters. Measured in
                // `ActiveMagicEffect.psc`'s `OnTrapHit`. It produces no token:
                // Papyrus is not newline-sensitive, so a continuation is the
                // same statement split purely for the page.
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j] == ' ' || bytes[j] == '\t' || bytes[j] == '\r') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == '\n' {
                    line = line.saturating_add(1);
                    i = j + 1;
                } else {
                    out.push(Token {
                        kind: Kind::Unknown('\\'),
                        line,
                        col: (tok_start - last_nl + 1) as u16,
                    });
                    i += 1;
                }
            }
            '{' => {
                let start_line = line;
                let start_col = (tok_start - last_nl + 1) as u16;
                i += 1;
                let mut body = String::new();
                while i < bytes.len() && bytes[i] != '}' {
                    if bytes[i] == '\n' {
                        line = line.saturating_add(1);
                    }
                    body.push(bytes[i]);
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1; // the closing brace
                }
                out.push(Token {
                    kind: Kind::DocComment(body.trim().to_string()),
                    line: start_line,
                    col: start_col,
                });
            }
            '"' => {
                let start_line = line;
                let start_col = (tok_start - last_nl + 1) as u16;
                i += 1;
                let mut s = String::new();
                while i < bytes.len() && bytes[i] != '"' {
                    if bytes[i] == '\\' && i + 1 < bytes.len() {
                        i += 1;
                        let e = bytes[i];
                        s.push(match e {
                            'n' => '\n',
                            't' => '\t',
                            'r' => '\r',
                            '\\' => '\\',
                            '"' => '"',
                            other => other,
                        });
                    } else {
                        if bytes[i] == '\n' {
                            line = line.saturating_add(1);
                        }
                        s.push(bytes[i]);
                    }
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
                out.push(Token {
                    kind: Kind::Str(s),
                    line: start_line,
                    col: start_col,
                });
            }
            c if c.is_ascii_digit()
                && (bytes.get(i + 1) == Some(&'x') || bytes.get(i + 1) == Some(&'X')) =>
            {
                // A hexadecimal form id: `Game.GetForm(0X0001DA07)`. The
                // sources spell it `0X` as often as `0x`, and reading the `0`
                // as an int leaves `X0001DA07` as a stray word inside the
                // argument list.
                let start_line = line;
                let start_col = (tok_start - last_nl + 1) as u16;
                i += 2;
                let mut digits = String::new();
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                    digits.push(bytes[i]);
                    i += 1;
                }
                let v = i32::from_str_radix(&digits, 16).unwrap_or(0);
                out.push(Token {
                    kind: Kind::Int(v),
                    line: start_line,
                    col: start_col,
                });
            }
            c if c.is_ascii_digit() => {
                let start_line = line;
                let start_col = (tok_start - last_nl + 1) as u16;
                let mut text = String::new();
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == '.') {
                    // A second dot means a member access, not a fractional part.
                    if bytes[i] == '.' && text.contains('.') {
                        break;
                    }
                    text.push(bytes[i]);
                    i += 1;
                }
                if text.contains('.') {
                    out.push(Token {
                        kind: Kind::Float(text.parse().unwrap_or(0.0)),
                        line: start_line,
                        col: start_col,
                    });
                } else {
                    out.push(Token {
                        kind: Kind::Int(text.parse().unwrap_or(0)),
                        line: start_line,
                        col: start_col,
                    });
                }
            }
            c if c.is_alphabetic() || c == '_' => {
                let start_line = line;
                let start_col = (tok_start - last_nl + 1) as u16;
                let mut text = String::new();
                while i < bytes.len() && (bytes[i].is_alphanumeric() || bytes[i] == '_') {
                    text.push(bytes[i]);
                    i += 1;
                }
                out.push(Token {
                    kind: Kind::Word(text),
                    line: start_line,
                    col: start_col,
                });
            }
            _ => {
                let two: String = bytes[i..(i + 2).min(bytes.len())].iter().collect();
                let start_line = line;
                let start_col = (tok_start - last_nl + 1) as u16;
                let kind = match two.as_str() {
                    "==" => {
                        i += 2;
                        Kind::EqEq
                    }
                    "!=" => {
                        i += 2;
                        Kind::NotEq
                    }
                    ":=" => {
                        i += 2;
                        Kind::ColonAssign
                    }
                    "<=" => {
                        i += 2;
                        Kind::LtEq
                    }
                    ">=" => {
                        i += 2;
                        Kind::GtEq
                    }
                    "&&" => {
                        i += 2;
                        Kind::AndAnd
                    }
                    "||" => {
                        i += 2;
                        Kind::OrOr
                    }
                    _ => {
                        i += 1;
                        match c {
                            '(' => {
                                depth += 1;
                                Kind::LParen
                            }
                            ')' => {
                                depth = (depth - 1).max(0);
                                Kind::RParen
                            }
                            '[' => {
                                depth += 1;
                                Kind::LBracket
                            }
                            ']' => {
                                depth = (depth - 1).max(0);
                                Kind::RBracket
                            }
                            ',' => Kind::Comma,
                            '.' => Kind::Dot,
                            '=' => Kind::Assign,
                            ':' => Kind::Colon,
                            '<' => Kind::Lt,
                            '>' => Kind::Gt,
                            '+' => Kind::Plus,
                            '-' => Kind::Minus,
                            '*' => Kind::Star,
                            '/' => Kind::Slash,
                            '%' => Kind::Percent,
                            '!' => Kind::Bang,
                            other => Kind::Unknown(other),
                        }
                    }
                };
                out.push(Token {
                    kind,
                    line: start_line,
                    col: start_col,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<Kind> {
        lex(src).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn a_comment_produces_nothing() {
        assert_eq!(
            kinds("; a comment\nfloat x"),
            vec![Kind::Word("float".into()), Kind::Word("x".into())]
        );
    }

    #[test]
    fn an_inline_comment_ends_at_the_line() {
        assert_eq!(
            kinds("x = 1 ; trailing\nwhile"),
            vec![
                Kind::Word("x".into()),
                Kind::Assign,
                Kind::Int(1),
                Kind::Newline,
                Kind::Word("while".into())
            ]
        );
    }

    #[test]
    fn keywords_are_case_insensitive_at_the_parser() {
        // The lexer keeps the source's spelling; the parser lowercases.
        let t = lex("EndIf");
        assert_eq!(t[0].word().unwrap(), "endif");
        assert_eq!(lex("endIf")[0].word().unwrap(), "endif");
    }

    #[test]
    fn a_negative_number_is_two_tokens() {
        assert_eq!(kinds("-5"), vec![Kind::Minus, Kind::Int(5)]);
    }

    #[test]
    fn a_dot_after_a_number_is_member_access() {
        // `1.0` is a float. `1.foo` is not valid Papyrus (there is no member
        // access on a literal), so the lexer reads the digits and stops; the
        // dot then starts whatever follows. The point of the test is that the
        // float path does not swallow a dot that has no digits behind it.
        assert_eq!(kinds("1.0"), vec![Kind::Float(1.0)]);
        assert_eq!(
            kinds("1.foo"),
            vec![Kind::Float(1.0), Kind::Word("foo".into())],
            "the lexer reads `1.` as a float, which the parser then rejects \
             rather than silently treating as member access"
        );
    }

    #[test]
    fn a_doc_comment_is_its_own_token() {
        let t = lex("{ The briarheart object. }\narmor property x auto");
        assert_eq!(t[0].kind, Kind::DocComment("The briarheart object.".into()));
    }

    #[test]
    fn a_string_keeps_its_escapes() {
        assert_eq!(kinds(r#""a\"b""#), vec![Kind::Str("a\"b".into())]);
    }

    #[test]
    fn lines_are_counted() {
        // A newline is a token of its own at bracket depth 0 — that is what
        // lets the parser end a statement — so `a\nb\nc` is five tokens and
        // the newlines carry the lines they end.
        let t = lex("a\nb\nc");
        let lines: Vec<u16> = t.iter().map(|t| t.line).collect();
        assert_eq!(lines, vec![1, 1, 2, 2, 3]);
    }

    #[test]
    fn an_unknown_character_is_a_token_not_a_panic() {
        assert_eq!(kinds("@"), vec![Kind::Unknown('@')]);
    }
}

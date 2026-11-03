//! The lexer: source text to tokens.
//!
//! obscript is line-oriented. A statement is one line; a `Begin`/`End` pair
//! spans the lines between them. The lexer's job is to split each line into
//! the words, numbers, strings and punctuation the parser reads, and to say
//! which line and column each token starts at — the diagnostics need the
//! column, and the editor's completion needs the word being typed.
//!
//! It **never fails**. A character the lexer does not recognise becomes a
//! `Punct` token of its own rather than an error, because the editor runs the
//! lexer on text that is not valid obscript yet: a highlighter that stopped at
//! the first unrecognised character would paint the rest of the file plain.
//! Whether the tokens *mean* anything is the parser's question, not this one's.

use std::fmt;

/// What kind of token this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A keyword or a command name: letters, digits and `_`.
    Word,
    /// A numeric literal, with an optional sign and at most one `.`.
    Number,
    /// A double-quoted string, quotes included.
    String,
    /// A `;` comment, running to the end of its line.
    Comment,
    /// Any operator or punctuation: `==`, `+`, `(`, `,`.
    Punct,
    /// Whitespace, kept so a re-emit of the tokens reproduces the source.
    Space,
}

/// One token, with the span it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The token's own text, exactly as written.
    pub text: String,
    /// What kind of token this is.
    pub kind: Kind,
    /// The zero-based line the token starts on.
    pub line: usize,
    /// The zero-based column, counted in characters, the token starts on.
    pub col: usize,
}

impl Token {
    /// Whether this token is the word `word`, case-insensitively.
    ///
    /// obscript's keywords and command names are case-insensitive — `SetAV`,
    /// `setav` and `SETAV` all compile — so every comparison a parser makes is
    /// this one rather than `==`.
    pub fn is_word(&self, word: &str) -> bool {
        self.kind == Kind::Word && self.text.eq_ignore_ascii_case(word)
    }

    /// The token's text.
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.text)
    }
}

/// The two-character operators, matched before any single character.
///
/// Order matters within a pair only in that both characters are consumed
/// together: `==` is one token, never `=` `=`.
const TWO_CHAR_OPS: [&str; 6] = ["==", "!=", "<=", ">=", "&&", "||"];

/// Lex `source` into tokens.
///
/// The whitespace tokens are kept: the parser skips them, but the editor's
/// completion needs the gap between two words to know where a word being typed
/// ends, and a re-emit of the tokens has to reproduce the source exactly for
/// the highlighter's overlay to line up.
///
/// The source is walked as `char`s with a one-character lookahead, because
/// obscript has no multibyte tokens: `char` is the correct unit, and a byte
/// cursor would be a slower way to walk the same thing. The line and column
/// counters advance per character, so a column is a character count — which is
/// what the editor's caret reports.
pub fn lex(source: &str) -> Vec<Token> {
    let chars: Vec<char> = source.chars().collect();
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0usize;
    let mut line = 0usize;
    let mut col = 0usize;

    while i < chars.len() {
        let c = chars[i];
        let (start_line, start_col) = (line, col);

        // --- whitespace ---------------------------------------------------
        // A newline resets the column; every other whitespace character
        // advances it, which keeps a tab-completion's column meaningful.
        if c.is_whitespace() {
            let mut text = String::new();
            while i < chars.len() && chars[i].is_whitespace() {
                if chars[i] == '\n' {
                    line += 1;
                    col = 0;
                } else {
                    col += 1;
                }
                text.push(chars[i]);
                i += 1;
            }
            out.push(Token {
                text,
                kind: Kind::Space,
                line: start_line,
                col: start_col,
            });
            continue;
        }

        // --- comment ------------------------------------------------------
        // From `;` to the end of the line, the `;` included.
        if c == ';' {
            let mut text = String::new();
            while i < chars.len() && chars[i] != '\n' {
                text.push(chars[i]);
                i += 1;
                col += 1;
            }
            out.push(Token {
                text,
                kind: Kind::Comment,
                line: start_line,
                col: start_col,
            });
            continue;
        }

        // --- string -------------------------------------------------------
        // obscript has no escape sequences inside a string, so the next `"`
        // closes it. An unterminated one runs to the end of the line, which is
        // what the GECK reports as "Mismatched quotes" — the lexer does not
        // judge it, it just stops there.
        if c == '"' {
            let mut text = String::new();
            text.push(c);
            i += 1;
            col += 1;
            while i < chars.len() && chars[i] != '"' && chars[i] != '\n' {
                text.push(chars[i]);
                i += 1;
                col += 1;
            }
            if i < chars.len() && chars[i] == '"' {
                text.push('"');
                i += 1;
                col += 1;
            }
            out.push(Token {
                text,
                kind: Kind::String,
                line: start_line,
                col: start_col,
            });
            continue;
        }

        // --- number -------------------------------------------------------
        // A sign is part of a literal only at the beginning of an expression or
        // after an operator/comma/open-paren.
        // If the preceding non-whitespace token is an operand (Word, Number, `)` or `]`),
        // `+` or `-` is an infix operator!
        let last_non_space = out
            .iter()
            .rev()
            .find(|t| t.kind != Kind::Space && t.kind != Kind::Comment);
        let preceded_by_operand = match last_non_space {
            Some(t) => {
                t.kind == Kind::Word || t.kind == Kind::Number || t.text == ")" || t.text == "]"
            }
            None => false,
        };

        let is_sign_operator = (c == '-' || c == '+') && preceded_by_operand;

        // A `.` that follows an operand is the member separator of
        // `Ref.member`, not the start of a `.5` literal: `REPCONHQFreeform
        // .1stFloorAlarm` (written without the space in the source) would
        // otherwise lex as the word `REPCONHQFreeform` and the number
        // `.1stFloorAlarm`, losing the separator.
        let dot_is_separator = c == '.'
            && preceded_by_operand
            && i > 0
            && !chars[i - 1].is_whitespace();
        let starts_number = !is_sign_operator
            && !dot_is_separator
            && (c.is_ascii_digit()
                || ((c == '-' || c == '+' || c == '.')
                    && i + 1 < chars.len()
                    && (chars[i + 1].is_ascii_digit()
                        || (chars[i + 1] == '.'
                            && i + 2 < chars.len()
                            && chars[i + 2].is_ascii_digit()))));
        if starts_number {
            let mut text = String::new();
            // The sign, taken once at the front.
            if c == '-' || c == '+' {
                text.push(c);
                i += 1;
                col += 1;
            }
            while i < chars.len() {
                let d = chars[i];
                if d.is_ascii_digit() || d == '.' {
                    text.push(d);
                    i += 1;
                    col += 1;
                } else {
                    break;
                }
            }
            // A digit run followed by letters is a name, not a number.
            // Fallout's records are named that way — `1ECookCook`,
            // `3CBSCave1`, `1stFloorAlarm`, `2eLVBStation`,
            // `3ETopsPresidentialSuite` — and the source spells the record's
            // name, so the whole run is one word. Splitting it produced
            // `ECookCook is not a declared variable`, a message that blamed the
            // source for the lexer's cut (7 corpus units).
            if i < chars.len() && (chars[i].is_alphabetic() || chars[i] == '_') {
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    text.push(chars[i]);
                    i += 1;
                    col += 1;
                }
                out.push(Token {
                    text,
                    kind: Kind::Word,
                    line: start_line,
                    col: start_col,
                });
                continue;
            }
            out.push(Token {
                text,
                kind: Kind::Number,
                line: start_line,
                col: start_col,
            });
            continue;
        }

        // --- word ---------------------------------------------------------
        // Letters, digits, `_` and `.`: the `.` is in the set because a
        // reference's method call is one word to the parser (`PlayerREF.SetAV`
        // lexes as three words and the parser joins them), and a word ending in
        // `.` joins the next.
        if c.is_alphabetic() || c == '_' {
            let mut text = String::new();
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                text.push(chars[i]);
                i += 1;
                col += 1;
            }
            out.push(Token {
                text,
                kind: Kind::Word,
                line: start_line,
                col: start_col,
            });
            continue;
        }

        // --- punctuation --------------------------------------------------
        // The two-character operators first, so `==` is one token.
        let pair: String = chars[i..(i + 2).min(chars.len())].iter().collect();
        if TWO_CHAR_OPS.contains(&pair.as_str()) {
            out.push(Token {
                text: pair,
                kind: Kind::Punct,
                line: start_line,
                col: start_col,
            });
            i += 2;
            col += 2;
            continue;
        }
        out.push(Token {
            text: c.to_string(),
            kind: Kind::Punct,
            line: start_line,
            col: start_col,
        });
        i += 1;
        col += 1;
    }

    out
}

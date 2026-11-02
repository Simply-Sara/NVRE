//! The obscript parser: tokens to statements.
//!
//! obscript is line-oriented: a statement is one line, and a `Begin`/`End`
//! pair spans the lines between them. The parser walks the tokens the lexer
//! produced, groups them by line, and reads each line as one of five things:
//!
//! 1. A **declaration** — `short`, `long`, `float`, `ref` — which declares a
//!    local variable and emits no instruction of its own.
//! 2. A **`Begin` block** — `Begin <event>` … `End`.
//! 3. A **control-flow statement** — `set`, `if`, `else`, `elseif`, `return`.
//! 4. A **command call** — a command name, optionally after `Ref.`.
//! 5. The **`ScriptName`** statement, which opens every `SCPT`.
//!
//! # What the parser does not do
//!
//! It does not resolve names. A command's opcode, a local's index and a
//! reference's index in the record's `SCRO` list are the emitter's job, and
//! they need the record's own declarations to answer — which is why the
//! declarations are collected across the whole script before any statement is
//! emitted, so a variable declared inside a block is known everywhere.
//!
//! # Declarations come first
//!
//! The game's own compiler reads a script's declarations before it reads any
//! statement: six of the shipped units declare a variable inside a block, and
//! the variable is known everywhere in them. So this parser collects them in
//! a first pass over the whole source, and the emitter uses that table.

use crate::lex::{Kind, Token};

/// One statement, as the parser read it.
///
/// The expressions inside a statement are held as their token runs rather
/// than as a parsed tree, because the emitter writes them in postfix and the
/// postfix order is the tree: parsing it twice would only risk the two
/// disagreeing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// `ScriptName <name>` — the script's own name.
    ScriptName(String),
    /// `Begin <event>` — opens a block. The arguments after the event name are
    /// kept as their token run, because a block's parameters are event-specific
    /// tagged values.
    Begin {
        /// The event's name, as the source spells it.
        event: String,
        /// The arguments, as the tokens after the event name.
        args: Vec<Token>,
    },
    /// `End` — closes the innermost open block.
    End,
    /// `set <target> to <expr>`
    Set {
        /// The variable being assigned to, as its token.
        target: Token,
        /// The expression, as its token run.
        expr: Vec<Token>,
    },
    /// `if <expr>`
    If(Vec<Token>),
    /// `else`
    Else,
    /// `elseif <expr>`
    ElseIf(Vec<Token>),
    /// `endif` — the end of a conditional chain.
    EndIf,
    /// `return`
    Return,
    /// A command call: `[Ref.]Command args…`.
    Call {
        /// The reference it is called on, when there is one.
        on: Option<String>,
        /// The command's name, as the source spells it.
        command: String,
        /// The arguments, as their token run.
        args: Vec<Token>,
    },
    /// A line the parser could not read. Carried rather than dropped so the
    /// emitter can report it with its line number instead of silently
    /// producing a shorter body than the source asked for.
    Unknown {
        /// The line the statement was on, zero-based.
        line: usize,
        /// The line's text, for the error message.
        text: String,
    },
}

/// A local variable the script declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decl {
    /// The variable's name, as the source spells it.
    pub name: String,
    /// Whether it is an integer (`short`/`long`) or a float or reference.
    pub is_int: bool,
    /// The line the declaration was on.
    pub line: usize,
}

/// A parsed script: its declarations and its statements.
#[derive(Debug, Clone, Default)]
pub struct Script {
    /// The local variables, in declaration order.
    pub decls: Vec<Decl>,
    /// The statements, in source order.
    pub stmts: Vec<Stmt>,
}

/// The keywords that introduce a statement of the parser's own.
const KEYWORDS: &[&str] = &[
    "scn",
    "scriptname",
    "begin",
    "end",
    "if",
    "else",
    "elseif",
    "endif",
    "set",
    "let",
    "to",
    "while",
    "foreach",
    "loop",
    "return",
    "call",
    "continue",
    "break",
    "short",
    "long",
    "float",
    "ref",
    "reference",
    "eval",
    "function",
    "label",
    "goto",
];

/// The declaration keywords, and whether each declares an integer.
fn decl_kind(word: &str) -> Option<bool> {
    match word.to_ascii_lowercase().as_str() {
        "short" | "long" | "int" => Some(true),
        "float" | "ref" | "reference" => Some(false),
        _ => None,
    }
}

/// Parse `source` into a script.
///
/// The declarations are collected in a first pass over every line, and the
/// statements in a second, so a variable declared inside a block is known
/// everywhere — which is what the game's own compiler does.
pub fn parse(source: &str) -> Script {
    let tokens = crate::lex::lex(source);
    let lines = group_by_line(&tokens);

    // Pass one: every declaration in the script.
    let mut decls: Vec<Decl> = Vec::new();
    for (line, toks) in &lines {
        let words: Vec<&Token> = toks.iter().filter(|t| t.kind != Kind::Space).collect();
        if let Some(first) = words.first() {
            if let Some(is_int) = decl_kind(&first.text) {
                for name in words.iter().skip(1) {
                    if name.kind == Kind::Word {
                        decls.push(Decl {
                            name: name.text.clone(),
                            is_int,
                            line: *line,
                        });
                    }
                }
            }
        }
    }

    // Pass two: every statement.
    let mut stmts = Vec::new();
    for (line, toks) in &lines {
        stmts.extend(parse_line(*line, toks));
    }

    Script { decls, stmts }
}

/// Group tokens by the line they start on.
///
/// A line's tokens are the ones whose `line` field matches. The groups come
/// out in line order because the lexer emits tokens in source order.
fn group_by_line(tokens: &[Token]) -> Vec<(usize, Vec<Token>)> {
    let mut out: Vec<(usize, Vec<Token>)> = Vec::new();
    for t in tokens {
        if t.kind == Kind::Space {
            // Whitespace belongs to the line it is on, so a blank line is a
            // group of its own rather than merging into the next statement.
            match out.last_mut() {
                Some((last, group)) if *last == t.line => group.push(t.clone()),
                _ => out.push((t.line, vec![t.clone()])),
            }
            continue;
        }
        match out.last_mut() {
            Some((last, group)) if *last == t.line => group.push(t.clone()),
            _ => out.push((t.line, vec![t.clone()])),
        }
    }
    out
}

/// The words of a line, with whitespace dropped.
fn words_of(toks: &[Token]) -> Vec<&Token> {
    toks.iter()
        .filter(|t| t.kind != Kind::Space && t.kind != Kind::Comment)
        .collect()
}

/// Parse one line's tokens into zero or more statements.
///
/// Zero, because a comment-only or blank line is not a statement. More than
/// one, because a line the parser does not understand yields an `Unknown` and
/// nothing else.
fn parse_line(line: usize, toks: &[Token]) -> Vec<Stmt> {
    let words = words_of(toks);
    let Some(first) = words.first() else {
        return Vec::new();
    };

    // `ScriptName` / `scn` — the script's own name. The name is the word after
    // the keyword.
    if first.is_word("scriptname") || first.is_word("scn") {
        let name = words.get(1).map(|t| t.text.clone()).unwrap_or_default();
        return vec![Stmt::ScriptName(name)];
    }

    // A declaration: no statement of its own, the variable is already in the
    // script's table.
    if decl_kind(&first.text).is_some() {
        return Vec::new();
    }

    // `Begin <event> [args]` and `End`.
    if first.is_word("begin") {
        let event = words.get(1).map(|t| t.text.clone()).unwrap_or_default();
        let args = rest_of(&words, 2);
        return vec![Stmt::Begin { event, args }];
    }
    if first.is_word("end") {
        return vec![Stmt::End];
    }

    // Control flow.
    if first.is_word("set") || first.is_word("let") {
        // `set <target> to <expr>`: the target is the word after `set`, `to`
        // ends it, and the rest is the expression.
        let Some(target) = words.get(1) else {
            return vec![unknown(line, toks)];
        };
        let rest = &words[2..];
        // The `to` that separates the target from the expression.
        let after_to = rest
            .iter()
            .position(|t| t.is_word("to"))
            .map(|at| &rest[at + 1..])
            .unwrap_or(&[]);
        let expr = rest_of(after_to, 0);
        return vec![Stmt::Set {
            target: bare(target),
            expr,
        }];
    }
    if first.is_word("if") {
        return vec![Stmt::If(rest_of(&words, 1))];
    }
    if first.is_word("elseif") {
        return vec![Stmt::ElseIf(rest_of(&words, 1))];
    }
    if first.is_word("else") {
        return vec![Stmt::Else];
    }
    if first.is_word("endif") {
        return vec![Stmt::EndIf];
    }
    if first.is_word("return") {
        return vec![Stmt::Return];
    }

    // A command call, with or without a reference in front of it.
    //
    // The reference and the command are separated by a `.`, which the lexer
    // emits as its own token. The dot joins a word to the word that follows
    // it, whether or not the writer put a space after it: `rDoor.Activate` and
    // `rDoor. Activate` are the same statement, and the GECK accepts both.
    if let Some(dot_at) = words.iter().position(|t| t.text == ".") {
        // The reference is the word before the dot, and the command the word
        // after it. A dot at the start of a line has no reference in front of
        // it, and a dot anywhere else — inside a number, say — is not this.
        if dot_at == 0 {
            return vec![unknown(line, toks)];
        }
        let ref_tok = words.get(dot_at - 1);
        let cmd_tok = words.get(dot_at + 1);
        if let (Some(r), Some(c)) = (ref_tok, cmd_tok) {
            if r.kind == Kind::Word && c.kind == Kind::Word {
                return vec![Stmt::Call {
                    on: Some(r.text.clone()),
                    command: c.text.clone(),
                    args: rest_of(&words, dot_at + 2),
                }];
            }
        }
    }

    if first.kind == Kind::Word && !KEYWORDS.contains(&first.text.to_ascii_lowercase().as_str()) {
        return vec![Stmt::Call {
            on: None,
            command: first.text.clone(),
            args: rest_of(&words, 1),
        }];
    }

    vec![unknown(line, toks)]
}

/// The tokens after `skip`, as a fresh vector with their positions zeroed.
///
/// A statement's tokens are matched by text and kind, never by where they sat
/// in the source — the emitter reads them in order, and a diagnostic reports
/// the statement's own line rather than a token's column. Zeroing the position
/// here means a `Stmt` carries only what it is read for, and two statements
/// that differ only in where their tokens sat compare equal.
fn rest_of(words: &[&Token], skip: usize) -> Vec<Token> {
    words
        .iter()
        .skip(skip)
        .map(|t| {
            let mut t = (*t).clone();
            t.line = 0;
            t.col = 0;
            t
        })
        .collect()
}

/// A copy of `t` with its position zeroed, for the one token a statement holds
/// on its own.
fn bare(t: &Token) -> Token {
    let mut t = t.clone();
    t.line = 0;
    t.col = 0;
    t
}

/// An unreadable line, carried with its text for the error message.
fn unknown(line: usize, toks: &[Token]) -> Stmt {
    let text: String = toks.iter().map(|t| t.text.as_str()).collect();
    Stmt::Unknown {
        line,
        text: text.trim().to_string(),
    }
}

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
        /// The reference target it is assigned on (when `Ref.var`), if any.
        on: Option<String>,
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
    /// A variable declaration: `short x`, `float x`, `ref x`. Emits no
    /// instruction of its own — the variable lives in the record's `SLSD`/
    /// `SCVR` list — but it is still a statement in the source, and the
    /// game's `jumpOps` counts it as one.
    Decl,
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
    /// What the declaration says it is.
    pub kind: DeclKind,
    /// The line the declaration was on.
    pub line: usize,
}

/// The kind of a declared variable.
///
/// The `s`/`f` tag follows it, and the reference-index rule counts a `ref`
/// but not a `float` — so the two must be distinguishable rather than folded
/// into one "not an int".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclKind {
    /// `short`/`long`/`int`
    Int,
    /// `float`
    Float,
    /// `ref`/`reference`
    Ref,
}

/// One variable of the record's own table: its name and the ID its `SLSD`
/// carries.
///
/// The bytecode's variable index is this ID, not the variable's position in
/// the table — measured over all 4,902 tagged variable references in
/// `FalloutNV.esm`'s `If`/`ElseIf` conditions: every index equals some
/// `SLSD`'s first word, and only 58% equal the table position. The GECK
/// assigns IDs with gaps (a variable added, removed and re-added keeps its
/// old ID), so the ID is only recoverable from the record, which is why the
/// compiler takes the table rather than inventing indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarEntry {
    /// The variable's name, as the source and the `SCVR` spell it.
    pub name: String,
    /// The `SLSD` ID — the index the bytecode's `s`/`f` tags carry.
    pub id: u16,
    /// Whether it is an integer (`short`/`long`/`int`) or a float/ref.
    pub is_int: bool,
}

/// A script this record's bytecode names but does not own: its variable
/// table, so a `Script.var` reference resolves to the var's own `SLSD` ID.
///
/// Measured: `VFactionSquad.bSquadChosen` compiles to
/// `72 <this record's ref index of VFactionSquad> 73 <bSquadChosen's ID in
/// VFactionSquad's SLSD>` — the ID is cross-record data the compiling record
/// does not carry, so the caller supplies the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteScript {
    /// The script's name, as the source spells it and as the SCRO entry
    /// names it.
    pub name: String,
    /// Its variables, with their `SLSD` IDs.
    pub vars: Vec<VarEntry>,
}

/// One slot of the record's reference list, in the order the record writes
/// its `SCRO` and `SCRV` subrecords.
///
/// The bytecode's reference index is the slot's 1-based position in that
/// list — measured, and the reason the list is data rather than derived:
/// the record writes `SCRO` (a named form) and `SCRV` (a declared `ref`
/// variable, bound to a placed form) subrecords interleaved, in the order
/// the GECK wrote them, and every reference index the bytecode carries —
/// `1C` prefixes, `72` expression prefixes, `5A`/`47` operands, command
/// arguments, `Begin` parameters — reads that one list. A model that counts
/// declared refs first and `SCRO` after fits only the records whose `SCRV`
/// entries happen to be written first (967 of 973 units; the other six,
/// and every unit the model mis-indexes, write `SCRO` before `SCRV`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefEntry {
    /// The name the source spells: the form's EDID for an `SCRO` slot, the
    /// declared variable's name for an `SCRV` slot.
    pub name: String,
    /// What the slot names: a placed form or global (`SCRO`), or a
    /// declared `ref` variable (`SCRV`).
    pub slot: RefSlot,
    /// For an `SCRO` slot, what the form is — a global variable (GLOB)
    /// writes `47` 'G' where a placed reference writes `5A` 'Z'.
    pub kind: RefKind,
}

/// Which subrecord a reference-list slot came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefSlot {
    /// An `SCRO` entry: a named form (placed reference, quest, global).
    Form,
    /// An `SCRV` entry: a declared `ref` variable, bound to a placed form.
    Var,
}

/// What an `SCRO` entry names, which decides the tag a standalone operand
/// writes for it.
///
/// Measured corpus-wide: a `20 47 <idx>` Global operand names a `GLOB`
/// record 360 of 365 times; a `20 5A <idx>` reference operand names a
/// placed form (`REFR`/`ACHR`/`ACRE`). The tag follows the entry's record
/// type, not its name — the six hardcoded global names the emitter used to
/// special-case were a guess, and globals the GECK spells differently
/// compiled with the wrong tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    /// A placed reference (REFR/ACHR/ACRE/...): `5A` as a standalone
    /// operand, `72` as a command argument.
    Reference,
    /// A global variable (GLOB): `47` as a standalone operand and as a
    /// `set` destination.
    Global,
}

/// A parsed script: its declarations, external references, and its statements.
#[derive(Debug, Clone, Default)]
pub struct Script {
    /// The local variables, in declaration order.
    pub decls: Vec<Decl>,
    /// The record's reference list, in the order the record writes its
    /// `SCRO` and `SCRV` subrecords. See [`RefEntry`].
    pub refs: Vec<RefEntry>,
    /// The record's own variable table, when the caller has it. Names here
    /// resolve to their `SLSD` IDs; a declared name absent from the table is
    /// a new variable, which the record's writer assigns an ID for.
    pub vars: Vec<VarEntry>,
    /// The scripts whose variables this record's bytecode names remotely.
    pub remotes: Vec<RemoteScript>,
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

/// The declaration keywords, and what each declares.
fn decl_kind(word: &str) -> Option<DeclKind> {
    match word.to_ascii_lowercase().as_str() {
        "short" | "long" | "int" => Some(DeclKind::Int),
        "float" => Some(DeclKind::Float),
        "ref" | "reference" => Some(DeclKind::Ref),
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
                            kind: is_int,
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

    Script {
        decls,
        refs: Vec::new(),
        vars: Vec::new(),
        remotes: Vec::new(),
        stmts,
    }
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

    // A declaration: no instruction of its own, the variable is in the
    // record's SLSD/SCVR list. Carried as its own statement, because the
    // game's jumpOps count includes it — a branch's skip count is the count
    // of its source statements, not of its emitted instructions.
    if decl_kind(&first.text).is_some() {
        return vec![Stmt::Decl];
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
        let after_set = &words[1..];
        let to_pos = after_set.iter().position(|t| t.is_word("to"));
        let Some(to_at) = to_pos else {
            return vec![unknown(line, toks)];
        };
        let target_slice = &after_set[..to_at];
        let expr = rest_of(&after_set[to_at + 1..], 0);

        if target_slice.len() == 3 && target_slice[1].text == "." {
            let on = Some(target_slice[0].text.clone());
            let target = bare(target_slice[2]);
            return vec![Stmt::Set { on, target, expr }];
        } else if let Some(target) = target_slice.first() {
            let on = None;
            let target = bare(target);
            return vec![Stmt::Set { on, target, expr }];
        }
        return vec![unknown(line, toks)];
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

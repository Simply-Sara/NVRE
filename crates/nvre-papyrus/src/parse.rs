//! Papyrus source into a script model.
//!
//! # What the source says
//!
//! ```text
//! Scriptname DLC2ThirskDoorScript extends ObjectReference
//!
//! ObjectReference Property OutsideMarker auto
//! Scene Property ExileScene auto
//! Float Property SomeValue = 1.5 auto
//!
//! Event OnActivate(ObjectReference akActivator)
//!     if (IsActivationBlocked())
//!         if (akActivator == Game.GetPlayer())
//!             ; nothing
//!         else
//!             Activate(akActivator, true)
//!         endif
//!     endif
//! EndEvent
//! ```
//!
//! A script is a name, an optional parent, then a flat list of declarations and
//! bodies: properties, variables, functions, events and states, in any order.
//!
//! # `Function` and `Event` are the same declaration
//!
//! A function has a return type; an event has none and is `native`-less. The
//! compiled bytes do not distinguish them — both are a name, a return type, a
//! flag byte and a body — so the parser folds `event` into `function` with a
//! `None` return type. That is what the game's own compiler does: the
//! distinction matters to the CK, which refuses to call an event, and not to
//! the bytecode.
//!
//! # Declarations carry their doc comments
//!
//! A `{…}` after a property is that property's doc string, and it lands in the
//! string table as well as in the property record. The parser attaches it
//! rather than dropping it, because a rewrite that loses it changes the string
//! table's indices and therefore every following reference in the file.

use crate::lex::{Kind, Token};
use crate::Error;

/// A type name as the source spelled it, e.g. `ObjectReference`, `float`,
/// `int[]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeName(pub String);

impl TypeName {
    /// The type with array brackets and whitespace removed, lowercased.
    ///
    /// `Float[]`, `float []` and `FLOAT` are one type to the compiler.
    pub fn base(&self) -> String {
        self.0
            .trim()
            .trim_end_matches("[]")
            .trim()
            .to_ascii_lowercase()
    }

    /// Whether the type is an array.
    pub fn is_array(&self) -> bool {
        self.0.trim().ends_with("[]")
    }
}

/// A declarator: `Actor Property Ref auto`.
#[derive(Debug, Clone, PartialEq)]
pub struct Declarator {
    /// The declared name.
    pub name: String,
    /// The array size if the source wrote one (`int Count = 4`), for a plain
    /// variable declaration.
    pub is_array: bool,
}

/// Kind of a top-level declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum DeclKind {
    /// `Float Property X auto` and its readable/writable forms.
    Property(PropertyDecl),
    /// `int Count` at script scope.
    Variable(VariableDecl),
    /// `Function`/`Event`, with or without a body.
    Function(FunctionDecl),
    /// `State Name … EndState`.
    State(StateDecl),
    /// `Struct Name … EndStruct` (Fallout 4).
    Struct(StructDecl),
}

/// One declaration at script scope.
#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    /// The declared type, as written.
    pub type_name: TypeName,
    /// The declaration's name.
    pub name: String,
    /// Its doc comment, if one was written.
    pub doc: Option<String>,
    /// Which kind it is.
    pub kind: DeclKind,
}

/// A property declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDecl {
    /// The declared type.
    pub type_name: TypeName,
    /// The property's name.
    pub name: String,
    /// `auto` — backed by a variable the compiler creates.
    pub is_auto: bool,
    /// `AutoReadOnly` — readable and not writable.
    pub is_read_only: bool,
    /// `conditional` — the property is only visible inside a conditional.
    pub is_conditional: bool,
    /// The initial value, when the source spelled one.
    pub initial: Option<Expr>,
    /// The ways the source spelled the property: `auto`, `AutoReadOnly`,
    /// or explicit `Function Get()`/`Function Set(…)` bodies.
    pub accessors: Option<PropertyAccessors>,
}

/// The explicit accessors of a non-auto property.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PropertyAccessors {
    /// The getter, when the source spelled one.
    pub getter: Option<FunctionDecl>,
    /// The setter, when the source spelled one.
    pub setter: Option<FunctionDecl>,
}

/// A script-scope variable.
#[derive(Debug, Clone, PartialEq)]
pub struct VariableDecl {
    /// The declared type.
    pub type_name: TypeName,
    /// Its name.
    pub name: String,
    /// Its initial value, when written (`int Count = 4`).
    pub initial: Option<Expr>,
}

/// A function or event.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    /// The declared return type; `None` for an event and for `Function`
    /// without a type, both of which mean `None`.
    pub return_type: Option<TypeName>,
    /// The function's name.
    pub name: String,
    /// Its parameters.
    pub params: Vec<Param>,
    /// Its locals, in declaration order.
    pub locals: Vec<LocalDecl>,
    /// Its body, statement by statement.
    pub body: Vec<Stmt>,
    /// Whether the source wrote `native`. A native function has no body and
    /// the engine supplies it.
    pub is_native: bool,
    /// Whether the source wrote `global`. A global function has no `self`.
    pub is_global: bool,
    /// Its doc comment.
    pub doc: Option<String>,
    /// The source line the declaration starts on; the debug block needs the
    /// line of each instruction, and a statement's line is this token's line.
    pub line: u16,
}

/// One parameter of a function.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// Its type.
    pub type_name: TypeName,
    /// Its name.
    pub name: String,
    /// The default value, when the source spelled one (`int i = 1`).
    ///
    /// A default is a source-language convenience: the compiler fills the
    /// argument at each call site, and the `.pex` function record keeps the
    /// parameter's name and type only. Carried so the compiler can do that.
    pub default: Option<Expr>,
}

/// A local declared inside a body.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalDecl {
    /// Its type.
    pub type_name: TypeName,
    /// Its name.
    pub name: String,
    /// Its initial value, when written.
    pub initial: Option<Expr>,
    /// The line it is declared on. Without it the debug table records 0 for
    /// the `assign` that initialises it.
    pub line: u16,
}

/// One state's contents.
#[derive(Debug, Clone, PartialEq)]
pub struct StateDecl {
    /// The state's name; empty for `Auto State`.
    pub name: String,
    /// The functions that override the script's in this state.
    pub functions: Vec<FunctionDecl>,
}

/// A Fallout 4 struct.
#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    /// The struct's name.
    pub name: String,
    /// Its members.
    pub members: Vec<LocalDecl>,
}

/// A statement in a body.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// A local declaration: `int i = 0`.
    Local(LocalDecl),
    /// An assignment: `x = expr`.
    Assign {
        /// What is being written.
        target: Expr,
        /// The value.
        value: Expr,
        /// The source line.
        line: u16,
    },
    /// `a += b` and its friends, which desugar to an assignment.
    CompoundAssign {
        /// What is being written.
        target: Expr,
        /// The operator: `+`, `-`, `*`, `/`, `%`.
        op: String,
        /// The value.
        value: Expr,
        /// The source line.
        line: u16,
    },
    /// A call as a statement: `Activate(akActivator, true)`.
    /// A bare expression used as a statement, with the line it sits on.
    Expr(Expr, u16),
    /// `if cond … elseif cond … else … endif`.
    If {
        /// The branches, in source order. The first entry's condition is the
        /// `if`'s; a bare `else` is a branch with no condition.
        branches: Vec<Branch>,
        /// The source line of the `if`.
        line: u16,
    },
    /// `while cond … endwhile`.
    While {
        /// The loop condition.
        cond: Expr,
        /// The body.
        body: Vec<Stmt>,
        /// The source line.
        line: u16,
    },
    /// `return expr` — the expression may be `None`.
    Return {
        /// The returned value.
        value: Expr,
        /// The source line.
        line: u16,
    },
    /// `StateName.GotoState()` and the bare `GotoState(…)` are calls; this is
    /// the statement form the compiler writes as a `GotoState` call.
    Break {
        /// The source line.
        line: u16,
    },
}

/// One arm of an `if`.
#[derive(Debug, Clone, PartialEq)]
pub struct Branch {
    /// The condition; `None` for `else`.
    pub cond: Option<Expr>,
    /// The statements.
    pub body: Vec<Stmt>,
    /// The source line.
    pub line: u16,
}

/// An expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A name: a variable, property, parameter, state name or class name.
    Ident(String),
    /// An integer literal.
    Int(i32),
    /// A float literal.
    Float(f32),
    /// A string literal.
    Str(String),
    /// `true` / `false`.
    Bool(bool),
    /// `None`.
    None,
    /// `lhs.name` — a member access, which the compiler emits as a `propget`
    /// or as the receiver of a method call.
    Member(Box<Expr>, String),
    /// `f(args)` — a call on `self` or a free function.
    Call {
        /// The callee.
        callee: Box<Expr>,
        /// The arguments.
        args: Vec<Expr>,
        /// Each argument's name when the source wrote one, parallel to `args`.
        ///
        /// `Game.DisablePlayerControls(abMovement = false, abMenu = false, …)`
        /// names two of nine parameters, and the `.pex` still writes **all
        /// nine** — each named value at its declared position and the rest at
        /// their defaults. Dropping the name and passing positionally put
        /// `false` in `abMovement`'s slot and `false` in `abFighting`'s, so the
        /// call ran with four wrong booleans: a silent behaviour bug, not a
        /// numbering one. 384 such calls across 176 of the 14,301 shipped
        /// sources.
        names: Vec<Option<String>>,
    },
    /// `a + b` and the other binary operators.
    Binary {
        /// The operator as the source spelled it.
        op: String,
        /// The left operand.
        lhs: Box<Expr>,
        /// The right operand.
        rhs: Box<Expr>,
    },
    /// `-a` / `!a`.
    Unary {
        /// The operator.
        op: String,
        /// The operand.
        operand: Box<Expr>,
    },
    /// `a as Type` — an explicit cast.
    Cast(Box<Expr>, TypeName),
    /// `a[i]`.
    Index(Box<Expr>, Box<Expr>),
}

/// A parsed script.
#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    /// `Scriptname`.
    pub name: String,
    /// `extends`, if written.
    pub parent: Option<String>,
    /// Whether the `Scriptname` line said `Hidden`.
    pub declares_hidden: bool,
    /// Whether the `Scriptname` line said `Conditional`.
    pub declares_conditional: bool,
    /// The script's doc comment.
    pub doc: Option<String>,
    /// Everything declared, in source order.
    pub decls: Vec<Decl>,
    /// Whether the source said `Native` at script scope, which is a
    /// Fallout-4 and later idiom for a script with no body.
    pub is_native: bool,
    /// Classes named by an `Import` line, lowercased, in source order.
    ///
    /// `Import Utility` is a directive, not a declaration. Parsed as one it
    /// looks exactly like `Type Name` — so it became a **variable** named
    /// `Utility` of type `Import`, which both emitted a variable the game does
    /// not have and interned `Utility` into the string table. The `.pex` then
    /// holds the capitalised spelling, so a later `callstatic` that should read
    /// `utility` comes out as `Utility` because the table keeps the first
    /// spelling it saw.
    pub imports: Vec<String>,
}

/// One parse failure, with the position it was found at.
///
/// Separate from [`Error`] because a recovery parse can report **several**, and
/// an editor wants all of them at once rather than the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDiag {
    /// The 1-based source line.
    pub line: u16,
    /// The 1-based source column.
    pub col: u16,
    /// What went wrong.
    pub message: String,
}

/// A script and everything that went wrong parsing it.
///
/// `script` is always usable: declarations that parsed are present, ones that did
/// not are missing. That is what a live editor needs — a half-typed file must
/// still yield the rest of its declarations so they can be checked.
#[derive(Debug, Clone)]
pub struct Partial {
    /// Whatever parsed. Never `None`; the header is required and a file without
    /// one is rejected outright.
    pub script: Script,
    /// Every failure, in source order.
    pub errors: Vec<ParseDiag>,
}

/// Parse a token stream, returning the first error and no AST.
///
/// Strict entry point. For an editor, use [`parse_partial`], which recovers.
pub fn parse(tokens: &[Token]) -> Result<Script, Error> {
    let p = parse_partial(tokens);
    if let Some(e) = p.errors.first() {
        return Err(Error::ParseAt {
            line: e.line,
            col: e.col,
            message: e.message.clone(),
        });
    }
    Ok(p.script)
}

/// Parse a token stream, **recovering** from errors.
///
/// A declaration or statement that fails is recorded and skipped, and parsing
/// resumes at the next line boundary, so one broken line does not cost the whole
/// file. The header (`Scriptname`) is not recoverable: without it there is no
/// script, and a file that lacks one is still a hard error.
pub fn parse_partial(tokens: &[Token]) -> Partial {
    let mut p = Parser {
        t: tokens,
        i: 0,
        errors: Vec::new(),
    };
    match p.script() {
        Ok(script) => Partial {
            script,
            errors: p.errors,
        },
        Err(e) => {
            // Only the header can fail this way. Report it as a diagnostic and
            // hand back a stub, so callers have one shape to deal with.
            let (line, col, message) = match e {
                Error::ParseAt { line, col, message } => (line, col, message),
                other => (1, 1, other.to_string()),
            };
            Partial {
                script: Script {
                    name: String::new(),
                    parent: None,
                    declares_hidden: false,
                    declares_conditional: false,
                    doc: None,
                    decls: Vec::new(),
                    is_native: false,
                    imports: Vec::new(),
                },
                errors: vec![ParseDiag { line, col, message }],
            }
        }
    }
}

struct Parser<'a> {
    t: &'a [Token],
    i: usize,
    /// Failures collected during a recovery parse. Empty for a strict parse of
    /// valid input, which is the corpus.
    errors: Vec<ParseDiag>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.t.get(self.i)
    }

    /// Skip the statement separators the lexer emits at line ends.
    ///
    /// Called at every point where a new statement may begin: the top of a
    /// declaration list, the top of a body, and after each statement. It is
    /// deliberately *not* called inside expression parsing, because that is
    /// where a newline must stop the parse rather than be ignored.
    fn skip_newlines(&mut self) {
        while self.peek().map(|t| &t.kind) == Some(&Kind::Newline) {
            self.i += 1;
        }
    }

    fn line(&self) -> u16 {
        self.peek().map(|t| t.line).unwrap_or(0)
    }

    fn eat_word(&mut self, w: &str) -> bool {
        if self.peek().and_then(|t| t.word()).as_deref() == Some(w) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn eat(&mut self, k: &Kind) -> bool {
        if self.peek().map(|t| &t.kind) == Some(k) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn at_end(&self) -> bool {
        self.i >= self.t.len()
    }

    /// Report a failure **at the position it happened**.
    ///
    /// At end of file there is no token to point at, so the position comes from
    /// the last token read — which is where the source actually ran out. The old
    /// version said only "end of file" with no position at all, and the caller
    /// then reported line 1, so an unterminated string on line 6 was reported at
    /// line 1.
    fn err<T>(&self, what: &str) -> Result<T, Error> {
        let (line, col, found) = match self.peek() {
            Some(t) => (t.line, t.col, t.kind.to_string()),
            None => match self.t.last() {
                Some(t) => (t.line, t.col, "end of file".to_string()),
                None => (1, 1, "empty source".to_string()),
            },
        };
        Err(Error::ParseAt {
            line,
            col,
            message: format!("{what}, found {found}"),
        })
    }

    /// Record a failure and skip to the next line boundary.
    ///
    /// Recovery is at the **line**, which is the only synchronisation point the
    /// grammar offers: every statement ends at a newline outside brackets, so a
    /// failed construct cannot swallow the one after it. Progress is guaranteed —
    /// if the failure consumed nothing, one token is discarded — so a malformed
    /// file cannot spin here.
    fn recover(&mut self, e: Error, started_at: usize) {
        let (line, col, message) = match e {
            Error::ParseAt { line, col, message } => (line, col, message),
            other => (self.line(), 1, other.to_string()),
        };
        self.errors.push(ParseDiag { line, col, message });
        if self.i == started_at && self.i < self.t.len() {
            self.i += 1;
        }
        // Skip past the end of the current line, but **stop before a block
        // terminator**. `endif`/`endwhile`/`endfunction` are never valid statement
        // starts, and consuming one would make the enclosing block report "never
        // reaches endif" — turning one error into two, the second of them false.
        //
        // A `Newline` token is only emitted at bracket depth 0, so this cannot
        // stop in the middle of an argument list.
        while self.i < self.t.len() {
            if let Some(w) = self.t[self.i].word() {
                // An exact list, not a prefix test: `endurance = 5` starts with
                // "end" and is an ordinary statement.
                if matches!(
                    w.as_str(),
                    "endif"
                        | "endwhile"
                        | "endfunction"
                        | "endevent"
                        | "endstate"
                        | "endproperty"
                        | "endstruct"
                ) {
                    return;
                }
            }
            let is_nl = self.t[self.i].kind == Kind::Newline;
            self.i += 1;
            if is_nl {
                break;
            }
        }
        self.skip_newlines();
    }

    /// A failure that is really "the source ended before this construct did".
    ///
    /// Positioned at the LAST token rather than at the start of the construct:
    /// the useful place to look is where the file ran out, not where the `if`
    /// began. The message names the opening line so both are available.
    fn err_unterminated<T>(&self, message: String) -> Result<T, Error> {
        let (line, col) = match self.t.last() {
            Some(t) => (t.line, t.col),
            None => (1, 1),
        };
        Err(Error::ParseAt { line, col, message })
    }

    fn word(&mut self, what: &str) -> Result<String, Error> {
        match self.peek().map(|t| t.kind.clone()) {
            Some(Kind::Word(w)) => {
                self.i += 1;
                Ok(w)
            }
            _ => self.err(what),
        }
    }

    /// A doc comment at the current position, if any.
    fn doc(&mut self) -> Option<String> {
        self.skip_newlines();
        if let Some(Kind::DocComment(s)) = self.peek().map(|t| t.kind.clone()) {
            self.i += 1;
            Some(s)
        } else {
            None
        }
    }

    /// `Scriptname X extends Y`
    fn script(&mut self) -> Result<Script, Error> {
        let mut doc = self.doc();
        // `Scriptname` may be preceded by `Native` in Fallout 4's own scripts.
        let mut is_native = false;
        if self.peek().and_then(|t| t.word()).as_deref() == Some("native") {
            // Look ahead: `Native` as the first word means the script modifier;
            // as anything else it is a type name, and no shipped script starts
            // with a bare type name at file scope.
            is_native = true;
            self.i += 1;
        }
        if !self.eat_word("scriptname") {
            return self.err("a script must start with `Scriptname`");
        }
        let name = self.word("a script name")?;
        let parent = if self.eat_word("extends") {
            Some(self.word("a parent class name")?)
        } else {
            None
        };
        // Trailing script flags: `Scriptname X extends Y Hidden`, and Fallout
        // 4's `… Native Hidden`. They belong to the script line, and leaving
        // one to be read as a declaration makes the *next* line's first word a
        // declared name — which is a parse error two lines later, pointing at
        // the wrong line.
        let mut declares_hidden = false;
        let mut declares_conditional = false;
        loop {
            match self.peek().and_then(|t| t.word()).as_deref() {
                Some("hidden") => {
                    declares_hidden = true;
                    self.i += 1;
                }
                Some("conditional") => {
                    declares_conditional = true;
                    self.i += 1;
                }
                Some("mandatory") => {
                    self.i += 1;
                }
                Some("native") => {
                    is_native = true;
                    self.i += 1;
                }
                _ => break,
            }
        }
        if let Some(d) = self.doc() {
            doc = Some(d);
        }

        let mut decls = Vec::new();
        let mut imports = Vec::new();
        self.skip_newlines();
        while !self.at_end() {
            // A stray doc comment before a declaration belongs to it.
            let pending = self.doc();
            self.skip_newlines();
            if self.at_end() {
                break;
            }
            // `Import` is a directive: it names a class whose globals are in
            // scope, and contributes no declaration and no string-table entry.
            if self.peek().and_then(|t| t.word()).as_deref() == Some("import") {
                self.i += 1;
                imports.push(
                    self.word("a class name after `import`")?
                        .to_ascii_lowercase(),
                );
                continue;
            }
            // Recovery at DECLARATION level: one broken declaration must not
            // cost the rest of the file. That is what makes a half-typed script
            // still worth checking - the declarations that did parse are handed
            // to the semantic checks, so an error on line 6 does not hide an
            // error on line 20.
            let started_at = self.i;
            match self.declaration() {
                Ok(mut decl) => {
                    if decl.doc.is_none() {
                        decl.doc = pending;
                    }
                    decls.push(decl);
                }
                Err(e) => self.recover(e, started_at),
            }
        }
        Ok(Script {
            name,
            parent,
            declares_hidden,
            declares_conditional,
            doc,
            decls,
            is_native,
            imports,
        })
    }

    /// One top-level declaration.
    fn declaration(&mut self) -> Result<Decl, Error> {
        let first = self.word("a declaration")?;
        let lower = first.to_ascii_lowercase();

        // `State` and `Auto State` open a state block.
        if lower == "auto" {
            if self.eat_word("state") {
                return self.state_block();
            }
            return self.err("`auto` must be followed by `state` at file scope");
        }
        if lower == "state" {
            return self.state_block();
        }

        // `Function` / `Event`, with or without a return type.
        if lower == "function" || lower == "event" {
            let f = self.function_after_keyword(&lower, None, self.line())?;
            return Ok(Decl {
                type_name: TypeName(
                    f.return_type
                        .as_ref()
                        .map(|t| t.0.clone())
                        .unwrap_or_else(|| "None".into()),
                ),
                name: f.name.clone(),
                doc: f.doc.clone(),
                kind: DeclKind::Function(f),
            });
        }

        // `Struct Name … EndStruct`
        if lower == "struct" {
            let name = self.word("a struct name")?;
            let members = self.struct_members("endstruct")?;
            return Ok(Decl {
                type_name: TypeName(name.clone()),
                doc: None,
                name: name.clone(),
                kind: DeclKind::Struct(StructDecl { name, members }),
            });
        }

        // Otherwise it is `Type name` or `Type[] Property name`, and what
        // follows decides which: `Property`, `Function`, or neither.
        let mut type_name = TypeName(first);
        if self.eat(&Kind::LBracket) {
            if !self.eat(&Kind::RBracket) {
                return self.err("`[` without `]` at script scope");
            }
            type_name = TypeName(format!("{}[]", type_name.0));
        }

        // `Type Function Name(…)` and `Type Event Name(…)` — the first word
        // was a return type, not the declaration's name. Reading `Function` as
        // a name here is what made `Game.GetPlayer` look like a declared
        // variable called `GetPlayer`.
        if self.peek().and_then(|t| t.word()).as_deref() == Some("function") {
            self.i += 1;
            let f =
                self.function_after_keyword("function", Some(type_name.clone()), self.line())?;
            return Ok(Decl {
                type_name: type_name.clone(),
                name: f.name.clone(),
                doc: f.doc.clone(),
                kind: DeclKind::Function(f),
            });
        }

        // `Property` comes *between* the type and the name: `Float Property X`
        // is a property, `Float X` is a variable. Checking for the keyword
        // after the name — which is what this did first — reads `Float` as a
        // type and `Property` as a variable's name, and every property in the
        // corpus then fails on the `X` that follows.
        if self.eat_word("property") {
            let prop_name = self.word("a property name")?;
            // `Float Property X = 1.5 auto` puts the initial value *before* the
            // flags, which is the CK's own order: `Type Property Name [= value]
            // [auto|AutoReadOnly] [hidden|conditional]`.
            let mut initial = None;
            if self.eat(&Kind::Assign) {
                initial = Some(self.expr()?);
            }
            let mut is_auto = false;
            let mut is_read_only = false;
            let mut is_conditional = false;
            loop {
                if self.eat_word("auto") {
                    is_auto = true;
                } else if self.eat_word("autoreadonly") {
                    is_auto = true;
                    is_read_only = true;
                } else if self.eat_word("conditional") {
                    is_conditional = true;
                } else if self.eat_word("hidden") || self.eat_word("mandatory") {
                    // Neither changes the compiled bytes; both are accepted.
                } else {
                    break;
                }
            }
            run_hidden_flags(self);
            let doc = self.doc();

            // A property with no `auto` spells its accessors out, and they are
            // wrapped in `endProperty`:
            //
            //     int Property Lever02Solution
            //         {doc}
            //         int Function Get()
            //             return solutionB
            //         endFunction
            //         Function Set(int value)
            //             …
            //         endFunction
            //     endProperty
            //
            // The `endProperty` is what closes the declaration. Leaving it to
            // be read as the next declaration is what made `puzLeversControl`
            // and twelve other scripts fail at a line far from the property.
            let accessors = if is_auto {
                None
            } else {
                Some(self.property_accessors()?)
            };
            return Ok(Decl {
                type_name: type_name.clone(),
                name: prop_name.clone(),
                doc,
                kind: DeclKind::Property(PropertyDecl {
                    type_name,
                    name: prop_name,
                    is_auto,
                    is_read_only,
                    is_conditional,
                    initial,
                    accessors,
                }),
            });
        }

        let name = self.word("a declared name")?;

        // `Type name = value` at script scope is a variable. Its trailing
        // `conditional`/`hidden` flags are modifiers, not more statements:
        // `int __axisGuardedToOpen = 0 conditional` is one declaration.
        let initial = if self.eat(&Kind::Assign) {
            Some(self.expr()?)
        } else {
            None
        };
        run_hidden_flags(self);
        // A trailing array-size declaration is part of the type.
        Ok(Decl {
            type_name: type_name.clone(),
            name: name.clone(),
            doc: None,
            kind: DeclKind::Variable(VariableDecl {
                type_name,
                name,
                initial,
            }),
        })
    }

    /// One state block, with the `state` keyword already consumed.
    ///
    /// The caller consumes `state` (or `Auto State`) before calling, so this
    /// must NOT look for it again. It used to: `declaration` ate `state` as its
    /// first word, `state_block` then found no `state` keyword, and every plain
    /// `State Busy … endState` block came back with an **empty name** — silently
    /// merging into the default state. `Auto State` was unaffected, because
    /// `auto` is consumed first and `state` was still there to eat. Measured on
    /// `BladeTrapHit`, whose `state CannotHit` and `Auto state CanHit` both
    /// landed in the default state.
    fn state_block(&mut self) -> Result<Decl, Error> {
        let name = self.word("a state name")?;
        let mut functions = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_end() || self.eat_word("endstate") {
                break;
            }
            let _doc = self.doc();
            let first = self.word("a state member")?;
            let lower = first.to_ascii_lowercase();
            if lower == "function" || lower == "event" {
                functions.push(self.function_after_keyword(&lower, None, self.line())?);
            } else if lower == "struct" {
                let _n = self.word("a struct name")?;
                let _ = self.struct_members("endstruct")?;
            } else {
                // `Type name Property … auto` inside a state.
                let _type = TypeName(first);
                if self.eat_word("property") {
                    let _pn = self.word("a property name")?;
                    loop {
                        if !(self.eat_word("auto") || self.eat_word("autoreadonly")) {
                            break;
                        }
                    }
                    run_hidden_flags(self);
                } else if self.eat_word("function") {
                    functions.push(self.function_after_keyword("function", None, self.line())?);
                }
            }
        }
        Ok(Decl {
            type_name: TypeName("None".into()),
            name: name.clone(),
            doc: None,
            kind: DeclKind::State(StateDecl { name, functions }),
        })
    }

    /// The `Function Get()`/`Function Set(…)` bodies of a non-auto property,
    /// up to and including its `endProperty`.
    ///
    /// A property may spell only one of the two: `puzLevers03` has a getter and
    /// no setter, and the compiled property's flag byte records exactly that.
    fn property_accessors(&mut self) -> Result<PropertyAccessors, Error> {
        let mut out = PropertyAccessors::default();
        loop {
            self.skip_newlines();
            if self.at_end() || self.eat_word("endproperty") {
                return Ok(out);
            }
            let _doc = self.doc();
            let first = self.word("a property accessor")?;
            let lower = first.to_ascii_lowercase();
            let (keyword, return_type) = if lower == "function" {
                ("function".to_string(), None)
            } else if lower == "event" {
                ("event".to_string(), None)
            } else {
                // `int Function Get()` — the first word was the return type.
                let ty = TypeName(first);
                let kw = self.word("`Function` after a return type")?;
                (kw.to_ascii_lowercase(), Some(ty))
            };
            let f = self.function_after_keyword(&keyword, return_type, self.line())?;
            if f.name.eq_ignore_ascii_case("get") {
                out.getter = Some(f);
            } else if f.name.eq_ignore_ascii_case("set") {
                out.setter = Some(f);
            } else {
                return Err(Error::ParseAt {
                    line: f.line,
                    col: 1,
                    message: format!(
                        "a property accessor must be `Get` or `Set`, found `{}`",
                        f.name
                    ),
                });
            }
        }
    }

    fn struct_members(&mut self, terminator: &str) -> Result<Vec<LocalDecl>, Error> {
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_end() || self.eat_word(terminator) {
                break;
            }
            let _doc = self.doc();
            let member_line = self.line();
            let ty = TypeName(self.word("a member type")?);
            let name = self.word("a member name")?;
            let initial = if self.eat(&Kind::Assign) {
                Some(self.expr()?)
            } else {
                None
            };
            out.push(LocalDecl {
                type_name: ty,
                name,
                initial,
                line: member_line,
            });
        }
        Ok(out)
    }

    /// A function or event body, after its keyword.
    fn function_after_keyword(
        &mut self,
        keyword: &str,
        declared_type: Option<TypeName>,
        line: u16,
    ) -> Result<FunctionDecl, Error> {
        // `Function Name(…)` — the name, then the parameters.
        let name = self.word("a function name")?;
        let mut params = Vec::new();
        if self.eat(&Kind::LParen) {
            if !self.eat(&Kind::RParen) {
                loop {
                    // `int[]` arrives as `Word(int)` `LBracket` `RBracket`
                    // *before* the name, not after it.
                    let base = TypeName(self.word("a parameter type")?);
                    let ty = if self.eat(&Kind::LBracket) {
                        if !self.eat(&Kind::RBracket) {
                            return self.err("`[` without `]` in a parameter type");
                        }
                        TypeName(format!("{}[]", base.0))
                    } else {
                        base
                    };
                    let pname = self.word("a parameter name")?;
                    // `int i = 1` and `int i = None` are both defaults.
                    let default = if self.eat(&Kind::Assign) || self.eat(&Kind::ColonAssign) {
                        Some(self.expr()?)
                    } else {
                        None
                    };
                    params.push(Param {
                        type_name: ty,
                        name: pname,
                        default,
                    });
                    if !self.eat(&Kind::Comma) {
                        break;
                    }
                }
                if !self.eat(&Kind::RParen) {
                    return self.err("`)` after a parameter list");
                }
            }
        }
        let mut is_native = false;
        let mut is_global = false;
        loop {
            if self.eat_word("native") {
                is_native = true;
            } else if self.eat_word("global") {
                is_global = true;
            } else {
                break;
            }
        }
        let doc = self.doc();

        let mut locals = Vec::new();
        let mut body = Vec::new();
        if !is_native {
            let end_word = if keyword == "event" {
                "endevent"
            } else {
                "endfunction"
            };
            self.block_into(end_word, &mut locals, &mut body)?;
        }

        // An event's return type is None; a `Function` with no declared type
        // is None too.
        let return_type = if keyword == "event" {
            None
        } else {
            declared_type
        };
        Ok(FunctionDecl {
            return_type,
            name,
            params,
            locals,
            body,
            is_native,
            is_global,
            doc,
            line,
        })
    }

    /// Statements up to `end_word`, hoisting declarations into `locals`.
    fn block_into(
        &mut self,
        end_word: &str,
        locals: &mut Vec<LocalDecl>,
        body: &mut Vec<Stmt>,
    ) -> Result<(), Error> {
        loop {
            self.skip_newlines();
            if self.at_end() {
                break;
            }
            if self.peek().and_then(|t| t.word()).as_deref() == Some(end_word) {
                self.i += 1;
                return Ok(());
            }
            // Recovery at STATEMENT level, so one broken line inside a function
            // does not discard the function: the statements that did parse are
            // kept, and the editor can still check them.
            let started_at = self.i;
            match self.statement(locals) {
                Ok(s) => body.push(s),
                Err(e) => self.recover(e, started_at),
            }
        }
        self.err_unterminated(format!(
            "a body that starts before line {} never reaches `{end_word}`",
            self.line()
        ))
    }

    fn statement(&mut self, locals: &mut Vec<LocalDecl>) -> Result<Stmt, Error> {
        let line = self.line();
        let _doc = self.doc();

        // Nested `if`.
        if self.eat_word("if") {
            return self.if_statement(line);
        }
        if self.eat_word("while") {
            let cond = self.condition()?;
            let mut inner_locals = Vec::new();
            let mut body = Vec::new();
            self.block_into("endwhile", &mut inner_locals, &mut body)?;
            locals.extend(inner_locals);
            return Ok(Stmt::While { cond, body, line });
        }
        if self.eat_word("return") {
            // `return` with no value is a `return None`.
            let value = if self.at_end()
                // A bare `return` ends at the line, like any other statement.
                || self.peek().map(|t| &t.kind) == Some(&Kind::Newline)
                || self.peek().and_then(|t| t.word()).as_deref() == Some("endfunction")
                || self.peek().and_then(|t| t.word()).as_deref() == Some("endevent")
                || self.peek().and_then(|t| t.word()).as_deref() == Some("endif")
                || self.peek().and_then(|t| t.word()).as_deref() == Some("else")
                || self.peek().and_then(|t| t.word()).as_deref() == Some("elseif")
            {
                Expr::None
            } else {
                self.expr()?
            };
            return Ok(Stmt::Return { value, line });
        }
        if self.eat_word("break") {
            return Ok(Stmt::Break { line });
        }

        // A local declaration is `Type name` where the word is a known type and
        // the next token is a word. The compiler decides this by whether the
        // two-word prefix is a type it knows; the parser recognises the builtin
        // type names and otherwise treats the word as an expression.
        if let Some(Kind::Word(w)) = self.peek().map(|t| t.kind.clone()) {
            if is_array_declaration(self.t, self.i) {
                // `ObjectReference[] foundEnableList`.
                self.i += 1;
                self.i += 1; // `[`
                if !self.eat(&Kind::RBracket) {
                    return self.err("`[` without `]` in a local's type");
                }
                let ty = TypeName(format!("{}[]", w));
                let name = self.word("a local's name")?;
                let initial = if self.eat(&Kind::Assign) {
                    Some(self.expr()?)
                } else {
                    None
                };
                let d = LocalDecl {
                    type_name: ty,
                    name,
                    initial,
                    line,
                };
                locals.push(d.clone());
                return Ok(Stmt::Local(d));
            }
            if is_builtin_type(&w) || self.looks_like_declaration() {
                let ty = TypeName(w.clone());
                self.i += 1;
                let name = self.word("a local's name")?;
                let initial = if self.eat(&Kind::Assign) {
                    Some(self.expr()?)
                } else {
                    None
                };
                let d = LocalDecl {
                    type_name: ty,
                    name,
                    initial,
                    line,
                };
                locals.push(d.clone());
                return Ok(Stmt::Local(d));
            }
        }

        // Everything else is an assignment or an expression statement.
        //
        // A compound assignment has to be recognised *before* the expression
        // parser runs: `x += 1` lexes as `x` `+` `=`, and `expr()` happily
        // consumes the `+` as a binary operator and then stops at the `=`,
        // which is what produced "an expression, found =" for 461 files.
        let target = self.expr_no_compound()?;
        if self.eat(&Kind::Assign) {
            let value = self.expr()?;
            return Ok(Stmt::Assign {
                target,
                value,
                line,
            });
        }
        for (op, spelled) in [
            (Kind::Plus, "+"),
            (Kind::Minus, "-"),
            (Kind::Star, "*"),
            (Kind::Slash, "/"),
            (Kind::Percent, "%"),
        ] {
            if self.peek().map(|t| &t.kind) == Some(&op)
                && self.t.get(self.i + 1).map(|t| &t.kind) == Some(&Kind::Assign)
            {
                self.i += 2;
                let value = self.expr()?;
                return Ok(Stmt::CompoundAssign {
                    target,
                    op: spelled.to_string(),
                    value,
                    line,
                });
            }
        }
        Ok(Stmt::Expr(target, line))
    }

    /// Whether the current two tokens look like `Type name`.
    ///
    /// A word followed by a word is a declaration unless the first is a
    /// keyword the grammar reserves. `Game.GetPlayer()` is a word, a dot; a
    /// call is a word, a paren; so a word followed by a word is the only shape
    /// a declaration takes.
    fn looks_like_declaration(&self) -> bool {
        let Some(cur) = self.peek() else {
            return false;
        };
        let Kind::Word(w) = &cur.kind else {
            return false;
        };
        let lower = w.to_ascii_lowercase();
        if is_statement_keyword(&lower) {
            return false;
        }
        // The second word must not be a keyword either. `myCrimeGoldModified
        // as int` is a cast used as a statement — the only occurrence of a
        // bare cast in the corpus, in `WEBountyCollectorScript.psc` — and
        // reading `as` as a declared name makes the next line's `int` a type
        // with nothing to declare.
        match self.t.get(self.i + 1).and_then(|t| t.word()) {
            Some(next) => !is_statement_keyword(&next),
            None => false,
        }
    }

    fn if_statement(&mut self, line: u16) -> Result<Stmt, Error> {
        let mut branches = Vec::new();
        // The branch's line is the keyword's, not the line the condition ended
        // on. Reading it after `condition()` returns the *next* line, which put
        // every `jmpf` one line too low in the debug table: `Probe.psc`'s `if`
        // on line 4 recorded line 5.
        let mut branch_line = line;
        let mut cond = Some(self.condition()?);
        loop {
            let mut inner_locals = Vec::new();
            let mut body = Vec::new();
            loop {
                self.skip_newlines();
                if self.at_end() {
                    break;
                }
                match self.peek().and_then(|t| t.word()).as_deref() {
                    Some("endif") | Some("else") | Some("elseif") => break,
                    _ => body.push(self.statement(&mut inner_locals)?),
                }
            }
            branches.push(Branch {
                cond: cond.take(),
                body,
                line: branch_line,
            });
            match self.peek().and_then(|t| t.word()).as_deref() {
                Some("elseif") => {
                    branch_line = self.line();
                    self.i += 1;
                    cond = Some(self.condition()?);
                }
                Some("else") => {
                    branch_line = self.line();
                    self.i += 1;
                    // `else if` — the CK accepts it as an `else` whose body
                    // starts with an `if`.
                    cond = None;
                }
                Some("endif") => {
                    self.i += 1;
                    return Ok(Stmt::If { branches, line });
                }
                _ => {
                    return self
                        .err_unterminated(format!("an `if` on line {line} never reaches `endif`"));
                }
            }
        }
    }

    // ---- expressions, by precedence -------------------------------------

    /// `||` binds loosest.
    fn expr(&mut self) -> Result<Expr, Error> {
        self.or_expr()
    }

    /// The condition of an `if`, `elseif` or `while`.
    ///
    /// The CK's own sources write the condition in parentheses — `if (a == b)`
    /// — and the parens are part of the statement, not of the expression. The
    /// expression parser stops at the closing `)`, so without consuming it the
    /// statement after the condition begins with a stray `)` and every
    /// enclosing body fails to parse. Measured on `Q3.psc`-shaped input: the
    /// first nested `if` with a parenthesised condition and an assignment in
    /// its body.
    fn condition(&mut self) -> Result<Expr, Error> {
        // The statement's own parens are balanced: `if (a == b)`, `if ((a as B).C)`
        // and `if (x && (y))` all open with one `(` and close with the last
        // `)`. Consuming exactly that pair is what the grammar says, and it is
        // why the condition is read as an expression and then one `)` is
        // expected — the expression parser already balances the inner ones.
        let e = self.expr()?;
        if self.peek().map(|t| &t.kind) == Some(&Kind::RParen) {
            self.i += 1;
        }
        Ok(e)
    }

    /// An expression that stops before a compound assignment's operator.
    ///
    /// `x += 1` is three tokens, and only the statement parser knows the pair
    /// means an assignment. Parsing the target with the ordinary expression
    /// parser consumes the `+` first, so this one declines to treat a
    /// `+ - * / %` that is immediately followed by `=` as a binary operator.
    fn expr_no_compound(&mut self) -> Result<Expr, Error> {
        // Read the postfix chain first. If a compound operator follows it,
        // that chain is the assignment's target and the operator belongs to
        // the statement. Doing it the other way round — parse fully, then look
        // — does not work, because the full parse *fails* on `x += 1`: it
        // consumes `+` and then finds no right operand before the `=`.
        // Look ahead on a copy of the token index only: `postfix` advances
        // `self.i`, so probing with it and then rewinding leaves the parser
        // mid-expression. That is what made
        // `(DA05 as DA05QuestScript).PlayerSawFuneral = True` fail — the rewind
        // put the reader back at `DA05`, and the `(` before it was never
        // consumed, so the statement began one token late.
        let save = self.i;
        let probe = {
            let mut probe = Parser {
                t: self.t,
                errors: Vec::new(),
                i: self.i,
            };
            probe.postfix().map(|e| (e, probe.i)).ok()
        };
        if let Some((target, end)) = probe {
            if self.at_compound_operator_at(end) {
                self.i = end;
                return Ok(target);
            }
        }
        self.i = save;
        self.expr()
    }

    /// Whether the tokens at `i` are a compound operator.
    fn at_compound_operator_at(&self, i: usize) -> bool {
        let compound = matches!(
            self.t.get(i).map(|t| &t.kind),
            Some(Kind::Plus)
                | Some(Kind::Minus)
                | Some(Kind::Star)
                | Some(Kind::Slash)
                | Some(Kind::Percent)
        );
        compound && self.t.get(i + 1).map(|t| &t.kind) == Some(&Kind::Assign)
    }

    fn or_expr(&mut self) -> Result<Expr, Error> {
        let mut lhs = self.and_expr()?;
        while self.eat(&Kind::OrOr) {
            let rhs = self.and_expr()?;
            lhs = Expr::Binary {
                op: "||".into(),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn and_expr(&mut self) -> Result<Expr, Error> {
        let mut lhs = self.eq_expr()?;
        while self.eat(&Kind::AndAnd) {
            let rhs = self.eq_expr()?;
            lhs = Expr::Binary {
                op: "&&".into(),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn eq_expr(&mut self) -> Result<Expr, Error> {
        let mut lhs = self.rel_expr()?;
        loop {
            let op = match self.peek().map(|t| &t.kind) {
                Some(Kind::EqEq) => "==",
                Some(Kind::NotEq) => "!=",
                _ => break,
            };
            self.i += 1;
            let rhs = self.rel_expr()?;
            lhs = Expr::Binary {
                op: op.to_string(),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn rel_expr(&mut self) -> Result<Expr, Error> {
        let mut lhs = self.add_expr()?;
        loop {
            let op = match self.peek().map(|t| &t.kind) {
                Some(Kind::Lt) => "<",
                Some(Kind::LtEq) => "<=",
                Some(Kind::Gt) => ">",
                Some(Kind::GtEq) => ">=",
                _ => break,
            };
            self.i += 1;
            let rhs = self.add_expr()?;
            lhs = Expr::Binary {
                op: op.to_string(),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn add_expr(&mut self) -> Result<Expr, Error> {
        let mut lhs = self.mul_expr()?;
        loop {
            let op = match self.peek().map(|t| &t.kind) {
                Some(Kind::Plus) => "+",
                Some(Kind::Minus) => "-",
                _ => break,
            };
            self.i += 1;
            let rhs = self.mul_expr()?;
            lhs = Expr::Binary {
                op: op.to_string(),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn mul_expr(&mut self) -> Result<Expr, Error> {
        let mut lhs = self.unary()?;
        loop {
            let op = match self.peek().map(|t| &t.kind) {
                Some(Kind::Star) => "*",
                Some(Kind::Slash) => "/",
                Some(Kind::Percent) => "%",
                _ => break,
            };
            self.i += 1;
            let rhs = self.unary()?;
            lhs = Expr::Binary {
                op: op.to_string(),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, Error> {
        if self.eat(&Kind::Minus) {
            let operand = self.unary()?;
            return Ok(Expr::Unary {
                op: "-".into(),
                operand: Box::new(operand),
            });
        }
        if self.eat(&Kind::Bang) {
            let operand = self.unary()?;
            return Ok(Expr::Unary {
                op: "!".into(),
                operand: Box::new(operand),
            });
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, Error> {
        let mut e = self.primary()?;
        loop {
            // A statement ends at a newline: `a` on one line and `(b)` on the
            // next are two statements, not a call. Every continuation below
            // requires a token that cannot begin a statement, so stopping here
            // is what makes the lexer's `Newline` do its job.
            if self.peek().map(|t| &t.kind) == Some(&Kind::Newline) {
                break;
            }
            if self.eat(&Kind::Dot) {
                let name = self.word("a member name")?;
                e = Expr::Member(Box::new(e), name);
            } else if self.peek().map(|t| &t.kind) == Some(&Kind::LParen) {
                self.i += 1;
                let mut args = Vec::new();
                let mut names: Vec<Option<String>> = Vec::new();
                if !self.eat(&Kind::RParen) {
                    loop {
                        // `f(akSource := self)` names the argument. The name is
                        // kept, because it decides the POSITION the value lands
                        // in: the `.pex` writes every parameter of the
                        // declaration, so a named argument goes to its declared
                        // slot and the unnamed ones fill the rest in order.
                        // The CK's own sources use both `name := value` and
                        // `name = value` for a named argument: measured in
                        // `BYOHHouseFishHatcheryScript.psc`, whose
                        // `ShowGiftMenu(abGivingGift = true, …)` is three
                        // `=`-named arguments.
                        let mut name = None;
                        if let Some(Kind::Word(w)) = self.peek().map(|t| t.kind.clone()) {
                            if matches!(
                                self.t.get(self.i + 1).map(|t| &t.kind),
                                Some(Kind::ColonAssign) | Some(Kind::Assign)
                            ) {
                                name = Some(w);
                                self.i += 2;
                            }
                        }
                        args.push(self.expr()?);
                        names.push(name);
                        if !self.eat(&Kind::Comma) {
                            break;
                        }
                    }
                    if !self.eat(&Kind::RParen) {
                        return self.err("`)` after an argument list");
                    }
                }
                e = Expr::Call {
                    callee: Box::new(e),
                    args,
                    names,
                };
            } else if self.eat(&Kind::LBracket) {
                let idx = self.expr()?;
                if !self.eat(&Kind::RBracket) {
                    return self.err("`]` after an index");
                }
                e = Expr::Index(Box::new(e), Box::new(idx));
            } else if self.eat_word("as") {
                let ty = TypeName(self.word("a cast type")?);
                e = Expr::Cast(Box::new(e), ty);
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> Result<Expr, Error> {
        match self.peek().map(|t| t.kind.clone()) {
            Some(Kind::Int(i)) => {
                self.i += 1;
                Ok(Expr::Int(i))
            }
            Some(Kind::Float(x)) => {
                self.i += 1;
                Ok(Expr::Float(x))
            }
            Some(Kind::Str(s)) => {
                self.i += 1;
                Ok(Expr::Str(s))
            }
            Some(Kind::LParen) => {
                self.i += 1;
                let e = self.expr()?;
                if !self.eat(&Kind::RParen) {
                    return self.err("`)` after a parenthesised expression");
                }
                Ok(e)
            }
            // `New Bool[10]` — an array allocation. The bracketed size is not
            // an index, so it is read here rather than by `postfix`.
            Some(Kind::Word(w)) if w.eq_ignore_ascii_case("new") => {
                self.i += 1;
                let base = TypeName(self.word("an array's element type")?);
                if !self.eat(&Kind::LBracket) {
                    return self.err("`[` after `New` and an element type");
                }
                let size = if self.eat(&Kind::RBracket) {
                    // `New Bool[]` allocates an empty array.
                    Expr::Int(0)
                } else {
                    let e = self.expr()?;
                    if !self.eat(&Kind::RBracket) {
                        return self.err("`]` after an array's size");
                    }
                    e
                };
                Ok(Expr::Call {
                    callee: Box::new(Expr::Ident(format!("New {}", base.0))),
                    args: vec![size],
                    names: vec![None],
                })
            }
            Some(Kind::Word(w)) => {
                self.i += 1;
                let lower = w.to_ascii_lowercase();
                Ok(match lower.as_str() {
                    "true" => Expr::Bool(true),
                    "false" => Expr::Bool(false),
                    "none" => Expr::None,
                    _ => Expr::Ident(w),
                })
            }
            _ => self.err("an expression"),
        }
    }
}

/// The words that may not start or name a declaration.
///
/// A declaration is `Type name`, so a word that is itself a keyword can be
/// neither the type nor the name. `as` in that position is the one the corpus
/// actually turns on; the rest are here so a future statement form that reuses
/// one of these words cannot silently become a local variable.
fn is_statement_keyword(lower: &str) -> bool {
    matches!(
        lower,
        "if" | "else"
            | "elseif"
            | "endif"
            | "while"
            | "endwhile"
            | "return"
            | "break"
            | "new"
            | "as"
            | "istrue"
            | "true"
            | "false"
            | "none"
            | "and"
            | "or"
            | "not"
    )
}

/// Whether the tokens at `i` are `Type [ ] name` — an array declaration.
fn is_array_declaration(t: &[Token], i: usize) -> bool {
    matches!(t.get(i).map(|t| &t.kind), Some(Kind::Word(_)))
        && t.get(i + 1).map(|t| &t.kind) == Some(&Kind::LBracket)
        && t.get(i + 2).map(|t| &t.kind) == Some(&Kind::RBracket)
        && matches!(t.get(i + 3).map(|t| &t.kind), Some(Kind::Word(_)))
}

/// The type names Papyrus defines, lowercased.
pub fn is_builtin_type(w: &str) -> bool {
    matches!(
        w.to_ascii_lowercase().as_str(),
        "bool" | "int" | "float" | "string" | "var" | "none"
    )
}

/// Skip the `hidden` and `conditional` modifiers that follow a declaration.
fn run_hidden_flags(p: &mut Parser<'_>) {
    loop {
        if p.eat_word("hidden") || p.eat_word("conditional") || p.eat_word("mandatory") {
            continue;
        }
        break;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn partial(src: &str) -> Partial {
        parse_partial(&crate::lex::lex(src))
    }

    #[test]
    fn a_token_carries_its_column() {
        // `Self.Disable()` on line 3, indented by one tab: `Self` starts at
        // column 2 and `Disable` at 7.
        let toks = crate::lex::lex("Scriptname A\nFunction F()\n\tSelf.Disable()\nEndFunction\n");
        let self_tok = toks
            .iter()
            .find(|t| t.word().as_deref() == Some("self"))
            .expect("Self is a token");
        assert_eq!((self_tok.line, self_tok.col), (3, 2));
        let disable = toks
            .iter()
            .find(|t| t.word().as_deref() == Some("disable"))
            .expect("Disable is a token");
        assert_eq!((disable.line, disable.col), (3, 7));
    }

    #[test]
    fn a_strict_parse_still_rejects_a_broken_file() {
        // The recovery parse must not weaken the strict one: a caller that wants
        // an AST or an error still gets exactly that.
        assert!(parse(&crate::lex::lex(
            "Scriptname A\nFunction F()\n\tCount = )\nEndFunction\n"
        ))
        .is_err());
    }

    #[test]
    fn recovery_keeps_the_declarations_after_the_error() {
        // THE POINT of recovery: an error in the first function must not cost
        // the third one, because the editor still has to check it.
        let p = partial(
            "Scriptname A\nFunction Broken()\n\tCount = )\nEndFunction\nFunction Fine()\n\tSelf.Disable()\nEndFunction\n",
        );
        assert_eq!(p.errors.len(), 1, "{:?}", p.errors);
        let names: Vec<&str> = p
            .script
            .decls
            .iter()
            .filter(|d| matches!(d.kind, DeclKind::Function(_)))
            .map(|d| d.name.as_str())
            .collect();
        assert!(
            names.contains(&"Fine"),
            "the function after the error must survive: {names:?}"
        );
    }

    #[test]
    fn recovery_reports_a_position_not_line_one() {
        let p = partial("Scriptname A\nFunction F()\n\tCount = )\nEndFunction\n");
        assert_eq!(p.errors.len(), 1);
        assert_eq!(p.errors[0].line, 3, "{:?}", p.errors);
        assert_eq!(p.errors[0].col, 10, "{:?}", p.errors);
    }

    #[test]
    fn an_unterminated_string_is_reported_where_it_starts() {
        // The string swallows the rest of the file, so the failure surfaces at
        // end of input - but the position is the unterminated literal, which is
        // the thing the user has to fix.
        let p = partial("Scriptname A\nFunction F()\n\tDebug.Trace(\"oops\nEndFunction\n");
        assert!(!p.errors.is_empty());
        assert_eq!(p.errors[0].line, 3, "{:?}", p.errors);
        assert_eq!(p.errors[0].col, 14, "{:?}", p.errors);
    }

    #[test]
    fn recovery_does_not_swallow_the_block_terminator() {
        // If recovery ate the `endif`, the block would report "never reaches
        // endif" - a second, false error caused by the first.
        let p = partial(
            "Scriptname A\nFunction F()\n\tif 1 == 1\n\t\tCount = )\n\tendif\nEndFunction\n",
        );
        assert_eq!(p.errors.len(), 1, "{:?}", p.errors);
        assert!(
            !p.errors[0].message.contains("never reaches"),
            "recovery must not lose the terminator: {:?}",
            p.errors
        );
    }

    #[test]
    fn recovery_works_at_DECLARATION_level_too() {
        // The statement-level tests above are satisfied by statement recovery
        // alone: a bad *statement* inside a function is recovered there. This
        // case fails at `declaration()`, so only declaration-level recovery saves
        // the function that follows - and removing that recovery must break this
        // test, which is why it exists.
        let p = partial(
            "Scriptname A\nFloat Property\nFunction Fine()\n\tSelf.Disable()\nEndFunction\n",
        );
        assert!(!p.errors.is_empty(), "the bad declaration must be reported");
        let names: Vec<&str> = p
            .script
            .decls
            .iter()
            .filter(|d| matches!(d.kind, DeclKind::Function(_)))
            .map(|d| d.name.as_str())
            .collect();
        assert!(
            names.contains(&"Fine"),
            "the function after a broken DECLARATION must survive: {names:?} / {:?}",
            p.errors
        );
    }

    #[test]
    fn a_valid_file_reports_nothing() {
        let p = partial("Scriptname A\nFunction F()\n\tSelf.Disable()\nEndFunction\n");
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        assert_eq!(p.script.decls.len(), 1);
    }
}

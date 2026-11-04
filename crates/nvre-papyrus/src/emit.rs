//! Source to bytecode: the code generator.
//!
//! # The machine the compiler targets
//!
//! Papyrus's VM is a register machine with named storage and no expression
//! stack. Every instruction names its operands, and a computed value needs
//! somewhere to live, so the compiler allocates **temporaries** — locals named
//! `::temp0`, `::temp1`, … — one per intermediate result. That is visible in
//! every shipped script: `DLC2ThirskDoorScript.pex`'s `OnActivate` has
//! `::temp0` through `::temp5` for six intermediate values.
//!
//! A call whose result nobody wants writes into `::NoneVar`, the compiler's
//! throwaway. Measured: `Activate(akActivator, true)` as a statement writes
//! `callmethod Activate self ::temp4 akActivator true` — the result goes to a
//! temp rather than `::NoneVar` because the compiler assigns every call's
//! result a slot and lets the optimizer drop it later.
//!
//! # Shapes, measured from the shipped corpus
//!
//! ```text
//! assign        dest value
//! cast          dest value
//! cmp_eq        dest lhs rhs
//! callmethod    method self dest  <count> args…
//! callstatic    class function dest  <count> args…
//! callparent    function self dest  <count> args…
//! propget       dest object property
//! propset       object property value
//! return        value
//! jmp           ops
//! jmpt          cond ops
//! jmpf          cond ops
//! ```
//!
//! A jump's operand is a **relative offset from the jump instruction itself**:
//! the target index is `jump_index + operand`. Settled by a loop, where only
//! this reading lands the backward `jmp` on the `while` condition; the forward
//! jumps then land exactly on `instructions.len()`, the index past the last
//! instruction. The earlier note here said the count was from the instruction
//! *after* the jump, which is off by one in both directions.

use crate::opcodes;
use crate::parse::{
    Branch, Decl, DeclKind, Expr, FunctionDecl, PropertyDecl, Script, Stmt, TypeName,
};
use crate::pex::{Function, Instruction, StrIndex, Value};
use crate::Error;

/// The compile-time view of one script: what names exist and what they are.
///
/// Name resolution needs to know whether a bare word is a local, a parameter, a
/// property, a variable, a state name or a class name, because each compiles
/// differently. The table is built from the script's own declarations plus a
/// parent class name, which this crate does not resolve further: a call on the
/// parent (`Parent.SomeFunc()`) is `callparent`, and anything else unknown is a
/// global function the engine supplies.
pub struct Symbols {
    /// The script's own name.
    pub script: String,
    /// The parent class, `""` for none.
    pub parent: String,
    /// Local and script variables, by lowercased name.
    pub variables: Vec<(String, TypeName)>,
    /// Properties, by lowercased name.
    pub properties: Vec<(String, TypeName)>,
    /// State names.
    pub states: Vec<String>,
}

/// One function being built.
struct FnBuf {
    /// The instructions in order.
    instructions: Vec<Instruction>,
    /// The source line of each instruction, parallel to `instructions`. The
    /// debug block needs it, and it must stay in step, so both are appended by
    /// the same helper.
    lines: Vec<u16>,
    /// The locals table **as the file will list it**: temporaries and declared
    /// `local`s, and nothing else. The game's functions list no `self` and no
    /// parameters here — measured on `DLC2ThirskDoorScript`, whose `OnActivate`
    /// declares six `::tempN` and `::NoneVar` and nothing more.
    locals: Vec<(StrIndex, StrIndex)>,
    /// The emitter's own type environment, which is wider than `locals`: it
    /// also carries parameters, `self`, and anything else whose type a later
    /// instruction needs to know. This is what `value_type` consults; it is
    /// never written to the file.
    types: Vec<(StrIndex, StrIndex)>,
    /// NOTE: the counter of handed-out temporaries is **not** here. It lives on
    /// `Compiler`, because it is monotonic across the whole script and never
    /// resets between functions — measured with the real compiler on three
    /// identical one-temp functions, which came out `::temp0`, `::temp1` and
    /// `::temp2`. `free_temps` below *is* per-function: the reuse list starts
    /// empty for every function.
    /// Temporaries that have been released and can be handed out again.
    ///
    /// **Per function.** A script-wide free list was tried and measured worse
    /// (39,868 identical functions against 41,006), because a slot released in
    /// one function cannot legitimately be reused by the next: each function
    /// names its own locals. The counter stays script-wide regardless.
    free_temps: Vec<(StrIndex, StrIndex)>,
    /// Whether `::NoneVar` has been declared yet.
    none_var: Option<StrIndex>,
    /// Parameters, by lowercased name.
    params: Vec<(String, StrIndex)>,
    /// Locals the source declared, by lowercased name.
    declared: Vec<(String, StrIndex)>,
}

impl FnBuf {
    fn new() -> Self {
        FnBuf {
            instructions: Vec::new(),
            lines: Vec::new(),
            locals: Vec::new(),
            types: Vec::new(),
            free_temps: Vec::new(),
            none_var: None,
            params: Vec::new(),
            declared: Vec::new(),
        }
    }

    fn push(&mut self, ins: Instruction, line: u16) {
        self.instructions.push(ins);
        self.lines.push(line);
    }

    fn here(&self) -> usize {
        self.instructions.len()
    }
}

/// The compiler's state while it builds one script.
pub struct Compiler<'a> {
    script: &'a Script,
    /// The script's own names plus whatever the caller supplied about its
    /// parent.
    ///
    /// The emitter does not need it yet — every name it resolves is declared in
    /// this one file — but the parent's members are what a call like
    /// `Parent.SomeFunc()` has to be checked against, so it is carried from the
    /// start rather than threaded through every call site later.
    #[allow(dead_code)]
    symbols: Symbols,
    /// The string table being built, in first-use order.
    strings: Vec<String>,
    /// Per-lowercased-variable-name type, for choosing the arithmetic opcode.
    types: Vec<(String, TypeName)>,
    /// The user-flag names this script declares, interned in declaration order
    /// before the parent class because that is where the file puts them.
    ///
    /// `hidden` and `conditional` are the two a script can declare; Skyrim's
    /// every script carries them in the order its source spells them on the
    /// `Scriptname` line, which the parser records as flags rather than names.
    user_flag_names: Vec<String>,
    /// The declared return type of the function being compiled, if any.
    ///
    /// `return` is a **typed slot** like any other. Measured with the real
    /// compiler:
    ///
    /// ```text
    /// ObjectReference Function F() { return akActorProp }   (Actor property)
    ///   -> cast ::temp0 ::akActorProp_var ; return ::temp0
    /// Form Function F() { return akActorProp }
    ///   -> cast ::temp0 ::akActorProp_var ; return ::temp0
    /// Actor Function F() { return Game.GetPlayer() }
    ///   -> no cast; the call already yields an actor
    /// ```
    cur_return: Option<String>,
    /// How many temporaries the whole script has handed out.
    ///
    /// **Script-wide, not per function** — measured with the real compiler on
    /// three identical one-temp functions, which came out `::temp0`, `::temp1`
    /// and `::temp2`. It never resets between functions. The reuse list does:
    /// it starts empty for every function, so a function with no release of its
    /// own starts at the counter rather than at zero. That is exactly why a
    /// no-parameter function in the shipped corpus can begin at `::temp8`.
    next_temp: usize,
    /// Condition temporaries of the `if` construct currently being compiled,
    /// with the nesting depth at which the construct started.
    ///
    /// The CK does **not** free a condition temp at its `jmpf`. It frees every
    /// condition temp of the construct when the *outermost* `if` finishes, in
    /// allocation order — so the highest-numbered one is on top of the LIFO free
    /// list and comes back first. Measured with the real compiler:
    ///
    /// ```text
    /// three separate ifs      -> cond temps ::temp0 ::temp0 ::temp0
    /// if { if { } } then if   -> ::temp0 ::temp1, next cond ::temp1
    /// if / elseif then if     -> ::temp0 ::temp1, next cond ::temp1
    /// ```
    ///
    /// Freeing at each `jmpf` instead is what a shipping compiler would do, and
    /// it is wrong: it hands the second branch of an `elseif` the first branch's
    /// slot.
    cond_temps: Vec<StrIndex>,
    /// How many `if` constructs are open.
    if_depth: usize,
}

/// One property's compiled form.
pub struct CompiledProperty {
    /// The property's name.
    pub name: StrIndex,
    /// Its type.
    pub type_name: StrIndex,
    /// Its doc comment.
    pub doc: StrIndex,
    /// Its flags.
    pub flags: u8,
    /// The backing variable for an auto property.
    pub auto_var: Option<StrIndex>,
    /// The getter, when the source spelled one.
    pub getter: Option<Function>,
    /// The setter, when the source spelled one.
    pub setter: Option<Function>,
}

/// A script's compiled form, ready to be assembled into a `.pex`.
pub struct CompiledScript {
    /// The string table, in first-use order.
    pub strings: Vec<String>,
    /// The script's own variables.
    pub variables: Vec<(StrIndex, StrIndex, Value)>,
    /// Its properties.
    pub properties: Vec<CompiledProperty>,
    /// Its states, each with its functions. The default state has the empty
    /// name and holds the script's own bodies.
    pub states: Vec<(StrIndex, Vec<Function>)>,
    /// The debug entries, in file order: `(object, state, function, lines)`.
    ///
    /// One entry per function **including the injected `GetState` and
    /// `GotoState`**, whose own entries have empty line tables. The `lines`
    /// vector is parallel to the function's instructions and holds the
    /// 1-based source line each one came from.
    pub debug: Vec<(StrIndex, StrIndex, StrIndex, Vec<u16>)>,
}

impl<'a> Compiler<'a> {
    /// A compiler for `script`, whose parent class is `parent`.
    pub fn new(script: &'a Script, symbols: Symbols) -> Self {
        let mut c = Compiler {
            script,
            symbols,
            strings: Vec::new(),
            types: Vec::new(),
            next_temp: 0,
            cur_return: None,
            cond_temps: Vec::new(),
            if_depth: 0,
            user_flag_names: {
                // Both are always present, on all 14,026 shipped scripts —
                // including the 2,859 whose source declares neither. Only the
                // order varies, and it does not follow the source line:
                // `conditional` precedes `hidden` on 8,730 and follows it on
                // 5,296, with no correlation to what the `Scriptname` line
                // spells. That order is not decoded, so this takes the more
                // common one and is wrong on the 5,296.
                vec!["conditional".to_string(), "hidden".to_string()]
            },
        };
        for d in &script.decls {
            match &d.kind {
                DeclKind::Variable(v) => c
                    .types
                    .push((v.name.to_ascii_lowercase(), v.type_name.clone())),
                DeclKind::Property(p) => {
                    c.types
                        .push((p.name.to_ascii_lowercase(), p.type_name.clone()));
                    // The instruction stream never mentions a property by its
                    // source name: it uses the backing variable
                    // `::Name_var`. Registering only the source spelling left
                    // every property argument with no known type, so an
                    // `Armor` passed where a `Form` was wanted got no upcast.
                    c.types.push((
                        format!("::{}_var", p.name).to_ascii_lowercase(),
                        p.type_name.clone(),
                    ));
                }
                _ => {}
            }
        }
        c
    }

    /// Intern a string, **case-insensitively**.
    ///
    /// The Creation Kit keeps one entry per name regardless of case, and the
    /// spelling that survives is the first one it saw. Measured over the whole
    /// shipped corpus: **not one of the 14,026 scripts** has two entries that
    /// differ only in case (`tools/interning2.py` checks this). A `.pex`'s own
    /// `size` and index fields depend on it, so a case-sensitive table produces
    /// a file whose lengths disagree with the game's even when the code matches.
    ///
    /// The consequence is not cosmetic. Papyrus names are case-insensitive, and
    /// the CK reuses text it has already seen without comparing case, so a
    /// *string literal* can come back with the capitalisation of an earlier
    /// variable or literal: declare `string barfoo` and write `"BarFoo"` and the
    /// script's runtime value can be `"barfoo"`. The shipped scripts show the
    /// table is case-folded; they cannot show the literal-capture part, which
    /// only appears at run time.
    fn intern(&mut self, s: &str) -> StrIndex {
        if let Some(i) = self.strings.iter().position(|x| x.eq_ignore_ascii_case(s)) {
            return i as StrIndex;
        }
        self.strings.push(s.to_string());
        (self.strings.len() - 1) as StrIndex
    }

    fn type_of(&self, name: &str) -> Option<&TypeName> {
        self.types
            .iter()
            .find(|(n, _)| n == &name.to_ascii_lowercase())
            .map(|(_, t)| t)
    }

    /// Whether a name is an integer-typed value the compiler knows.
    fn is_int(&self, name: &str) -> bool {
        match self.type_of(name) {
            Some(t) => t.base() == "int",
            None => false,
        }
    }

    /// Compile the whole script.
    ///
    /// # The compiler injects two functions into every script
    ///
    /// `GetState` and `GotoState` are not in any source file — the compiler
    /// adds them, and they are why every shipped `.pex` has them at string
    /// indices 2 and 3 regardless of what its source says. Their bodies are
    /// fixed, measured on `DLC2ThirskDoorScript.pex`:
    ///
    /// ```text
    /// String GetState()
    ///     {Function that returns the current state}
    ///     return ::State
    ///
    /// None GotoState(String newState)
    ///     {Function that switches this object to the specified state}
    ///     callmethod onEndState self ::NoneVar  <0>
    ///     assign ::State newState
    ///     callmethod onBeginState self ::NoneVar  <0>
    /// ```
    ///
    /// They are emitted first in the default state, in that order, ahead of
    /// the script's own bodies.
    ///
    /// # The string table's order
    ///
    /// The table is the sequence of distinct strings the finished file
    /// *mentions*, read in file order — measured on 14,020 of the 14,026
    /// shipped scripts by `tools/interning2.py`. That makes the debug block
    /// the first thing to name anything, so this function interns in the
    /// file's field order rather than a convenient one:
    ///
    /// ```text
    /// 0        the script name
    /// 1        ""                       (the debug entries' state name)
    /// 2, 3     GetState, GotoState      (the injected pair's names)
    /// 4..      the script's own body names, in file order
    /// then     hidden/conditional, in that order — the object's user flags
    /// then     the parent class, then the object's own name (already 0), its
    ///          doc, its auto-state
    /// then     each variable: name, type
    /// then     each property: the backing variable first, then the name
    /// then     the states' bodies, in file order: return type, doc, params,
    ///          locals, operands
    /// ```
    pub fn compile(mut self) -> Result<CompiledScript, Error> {
        let script_name = self.script.name.clone();
        let _ = self.intern(&script_name);

        // The bodies to compile, in file order: named states, then the
        // script's own. The injected pair is separate and always first.
        // **Source order**, in one pass. This is not cosmetic: the temporary
        // counter is script-wide and advances in the order functions are
        // COMPILED, while the file lays them out in a different (hash) order.
        // `AchievementsScript` proves it — `OnTrackedStatsEvent` is line 43 and
        // starts at `::temp0`, `IncSideQuests` is line 154 and starts at
        // `::temp8`. Compiling named states first (what this did) put the
        // functions in the wrong order and put every later temporary out by the
        // amount the earlier ones had allocated.
        let mut order: Vec<(Option<String>, &FunctionDecl)> = Vec::new();
        for d in &self.script.decls {
            match &d.kind {
                DeclKind::State(st) => {
                    for f in &st.functions {
                        order.push((Some(st.name.clone()), f));
                    }
                }
                DeclKind::Function(f) => order.push((None, f)),
                _ => {}
            }
        }

        // ---- the debug block's names, in entry order ----
        let empty = self.intern("");
        let get_state = self.intern("GetState");
        let goto_state = self.intern("GotoState");
        let mut own_names: Vec<StrIndex> = Vec::new();
        for (_state, f) in &order {
            own_names.push(self.intern(&canonical_event_name(&f.name)));
        }
        let obj_name = self.intern(&script_name);

        // ---- the object's user flags, then its parent ----
        for flag in self.user_flag_names.clone() {
            let _ = self.intern(&flag);
        }
        let parent = self.script.parent.clone().unwrap_or_default();
        let _parent_idx = self.intern(&parent);

        // ---- the variables ----
        //
        // An `auto` property's storage is a **variable of the object**, not
        // part of the property record: `Quest Property DA10 auto` produces
        // `::DA10_var : quest` in the variable list. Omitting them was the
        // single largest content gap in the census, because most shipped
        // scripts declare most of their state this way.
        //
        // They are emitted before the source's own variables, intermixed into
        // one list (see `achievementsscript`, whose `::HardWorkerDone_var` sits
        // among the real variables rather than after them).
        //
        // The order *within* the list is the Creation Kit's hash-container
        // iteration, which is not decoded: measured over the 4,336 scripts that
        // declare two or more `auto` properties, file order equals source order
        // in 864, reversed source order in 671, and neither in 2,801. So this
        // emits source order and is knowingly wrong for the rest — `semcensus`
        // sorts before comparing and so does not see the difference.
        let mut variables = Vec::new();
        let autos: Vec<&PropertyDecl> = self
            .script
            .decls
            .iter()
            .filter_map(|d| match &d.kind {
                // `AutoReadOnly` is NOT backed by a variable: its value is a
                // constant folded into the getter. Measured on `actor.pex`,
                // whose five `CritStage_* AutoReadOnly` properties leave the
                // object's variable list completely empty.
                DeclKind::Property(p) if p.is_auto && !p.is_read_only => Some(p),
                _ => None,
            })
            .collect();
        for p in autos.iter().rev() {
            let n = self.intern(&format!("::{}_var", p.name));
            let t = self.var_type(&p.type_name.0);
            let init = match &p.initial {
                Some(e) => self.value_of(Some(e), &p.type_name)?,
                None => Value::None,
            };
            variables.push((n, t, init));
        }
        for d in &self.script.decls {
            if let DeclKind::Variable(v) = &d.kind {
                let n = self.intern(&v.name);
                let t = self.var_type(&v.type_name.0);
                let init = self.value_of(v.initial.as_ref(), &v.type_name)?;
                variables.push((n, t, init));
            }
        }

        // ---- the properties, backing variable before name ----
        let mut properties = Vec::new();
        for d in &self.script.decls {
            if let DeclKind::Property(p) = &d.kind {
                properties.push(self.property(p)?);
            }
        }
        // Records are written in ascending **string index** order, which is
        // measured rather than assumed: over the corpus, property name indices
        // ascend in 4,399 of 4,420 scripts (99.5%) and variable name indices in
        // 4,519 of 4,524 (99.9%). That is not a hash order — it falls out of
        // the interning walk, and it means sorting by index reproduces it.
        //
        // Functions and states are the exceptions: their indices ascend in only
        // 20.8% and 53.2% respectively, so those genuinely are hash-ordered and
        // stay in source order here.
        properties.sort_by_key(|p| p.name);
        variables.sort_by_key(|(n, _, _)| *n);

        // ---- the bodies, in file order, in the default state ----
        let mut debug: Vec<(StrIndex, StrIndex, StrIndex, Vec<u16>)> = Vec::new();
        let mut default_funcs = Vec::new();

        // The injected pair. Their names are already interned; their bodies
        // introduce `String`, the doc strings, `::State` and so on in the order
        // the file mentions them, which is why they are compiled first.
        default_funcs.push(self.get_state_fn(get_state, empty));
        default_funcs.push(self.goto_state_fn(goto_state, empty));
        // The injected pair has **real** debug entries with its own names. The
        // block is not just the source's own bodies: a script declaring no
        // functions still lists both, and `T4.pex`'s block has six entries for
        // four source functions.
        debug.push((obj_name, empty, get_state, Vec::new()));
        debug.push((obj_name, empty, goto_state, Vec::new()));

        let mut states: Vec<(StrIndex, Vec<Function>)> = Vec::new();
        let mut index = 0usize;
        let mut by_state: Vec<(Option<String>, Vec<Function>)> = Vec::new();
        for (state, f) in &order {
            let (cf, lines) = self.function(f)?;
            let _ = own_names[index];
            debug.push((
                obj_name,
                self.intern(state.as_deref().unwrap_or("")),
                own_names[index],
                lines,
            ));
            index += 1;
            match by_state.iter_mut().find(|(k, _)| k == state) {
                Some((_, v)) => v.push(cf),
                None => by_state.push((state.clone(), vec![cf])),
            }
        }
        // The default state is **always present and always first**, and it is
        // the only state that carries the injected pair. Measured over all
        // 14,026 shipped scripts:
        //
        //   13,323  have the default state alone
        //      703  have the default state *and* named states
        //        0  have only named states
        //      703  of 703 multi-state scripts put the default state first
        //
        // Emitting the default state only when the source had a top-level body
        // dropped `GetState` and `GotoState` from every script whose functions
        // all live inside named states — 138 scripts, and the resulting `.pex`
        // does not load. A script with no top-level body still has the pair.
        let named = self.intern("");
        for (name, mut funcs) in by_state {
            let n = self.intern(name.as_deref().unwrap_or(""));
            let is_default = name.is_none();
            if is_default {
                // The script's own bodies go after the injected pair.
                let mut all = std::mem::take(&mut default_funcs);
                all.append(&mut funcs);
                funcs = all;
            }
            if is_default {
                states.insert(0, (n, funcs));
            } else {
                states.push((n, funcs));
            }
        }
        if !states.iter().any(|(n, _)| *n == named) {
            states.insert(0, (named, std::mem::take(&mut default_funcs)));
        }

        Ok(CompiledScript {
            strings: self.strings,
            variables,
            properties,
            states,
            debug,
        })
    }

    /// The injected `String GetState()`.
    fn get_state_fn(&mut self, name: StrIndex, _empty: StrIndex) -> Function {
        let ret = self.intern("String");
        let doc = self.intern("Function that returns the current state");
        let state_var = self.intern("::State");
        let mut ins = Instruction::new(opcode_of("return"));
        ins.args.push(Value::Identifier(state_var));
        Function {
            name,
            return_type: ret,
            doc,
            user_flags: 0,
            flags: 0,
            params: Vec::new(),
            // `::State` is a script-level variable, not a local: the game's
            // `GetState` has an *empty* locals table. Measured on
            // DLC2ThirskDoorScript.
            locals: Vec::new(),
            instructions: vec![ins],
        }
    }

    /// The injected `Function GotoState(String newState)`.
    fn goto_state_fn(&mut self, name: StrIndex, _empty: StrIndex) -> Function {
        let ret = self.intern("None");
        let doc = self.intern("Function that switches this object to the specified state");
        let new_state = self.intern("newState");
        let string_t = self.intern("String");
        let on_end = self.intern("onEndState");
        let self_i = self.intern("self");
        let none_var = self.intern("::NoneVar");
        let state_var = self.intern("::State");
        let on_begin = self.intern("onBeginState");

        let mut call_end = Instruction::new(opcode_of("callmethod"));
        call_end.args.push(Value::Identifier(on_end));
        call_end.args.push(Value::Identifier(self_i));
        call_end.args.push(Value::Identifier(none_var));

        let mut assign = Instruction::new(opcode_of("assign"));
        assign.args.push(Value::Identifier(state_var));
        assign.args.push(Value::Identifier(new_state));

        let mut call_begin = Instruction::new(opcode_of("callmethod"));
        call_begin.args.push(Value::Identifier(on_begin));
        call_begin.args.push(Value::Identifier(self_i));
        call_begin.args.push(Value::Identifier(none_var));

        Function {
            name,
            return_type: ret,
            doc,
            user_flags: 0,
            flags: 0,
            params: vec![crate::pex::TypedName {
                name: new_state,
                type_name: string_t,
            }],
            locals: vec![crate::pex::TypedName {
                name: none_var,
                type_name: ret,
            }],
            instructions: vec![call_end, assign, call_begin],
        }
    }

    fn property(&mut self, p: &PropertyDecl) -> Result<CompiledProperty, Error> {
        let name = self.intern(&p.name);
        // A property's type slot is spelled like a variable's: primitives
        // capitalised, classes lowercased.
        let type_name = self.var_type(&p.type_name.0);
        let doc = self.intern("");
        let mut flags = 0u8;
        let mut auto_var = None;
        let mut getter = None;
        let mut setter = None;

        if p.is_auto && p.is_read_only {
            // `AutoReadOnly` is a different shape from plain `auto`, and the
            // difference is structural rather than cosmetic — measured by
            // compiling one with the real compiler:
            //
            //   * no backing variable at all (`::Name_var` is not created)
            //   * flag bit 2 (auto) is NOT set; it carries only bit 0
            //   * it has a real getter body returning the constant
            //
            // So it takes the same path as a hand-written accessor.
            flags |= 0x1;
            let mut ins = Instruction::new(opcode_of("return"));
            ins.args
                .push(self.value_of(p.initial.as_ref(), &p.type_name)?);
            getter = Some(Function {
                name: self.intern("Get"),
                return_type: type_name,
                doc: self.intern(""),
                user_flags: 0,
                flags: 0,
                params: Vec::new(),
                locals: Vec::new(),
                instructions: vec![ins],
            });
        } else if p.is_auto {
            // Plain `auto`: readable and writable, backed by `::Name_var`.
            flags |= 0x4;
            flags |= 0x3;
            let var = self.intern(&format!("::{}_var", p.name));
            auto_var = Some(var);
        } else if let Some(a) = &p.accessors {
            if let Some(g) = &a.getter {
                flags |= 0x1;
                let (cf, _) = self.function(g)?;
                getter = Some(cf);
            }
            if let Some(s) = &a.setter {
                flags |= 0x2;
                let (cf, _) = self.function(s)?;
                setter = Some(cf);
            }
        }
        Ok(CompiledProperty {
            name,
            type_name,
            doc,
            flags,
            auto_var,
            getter,
            setter,
        })
    }

    fn value_of(&mut self, e: Option<&Expr>, ty: &TypeName) -> Result<Value, Error> {
        let Some(e) = e else {
            return Ok(match ty.base().as_str() {
                "int" | "float" => Value::None,
                _ => Value::None,
            });
        };
        Ok(match e {
            Expr::Int(i) => Value::Int(*i),
            Expr::Float(f) => Value::Float(*f),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Str(s) => Value::String(self.intern(s)),
            Expr::None => Value::None,
            Expr::Ident(n) => Value::Identifier(self.intern(n)),
            // A negative literal is a unary minus over a literal, not a
            // literal, so `Int Property X = -1 auto` used to be refused
            // outright: 94 of the shipped scripts do exactly that.
            Expr::Unary { op, operand } if op == "-" => match self.value_of(Some(operand), ty)? {
                Value::Int(i) => Value::Int(-i),
                Value::Float(f) => Value::Float(-f),
                other => other,
            },
            other => {
                return Err(Error::Parse(format!(
                    "a property's initial value must be a literal, found {other:?}"
                )))
            }
        })
    }

    /// Compile one function body.
    fn function(&mut self, f: &FunctionDecl) -> Result<(Function, Vec<u16>), Error> {
        let name = self.intern(&canonical_event_name(&f.name));
        // A return type slot is NOT spelled like a variable slot: measured over
        // the corpus, a class return type is capitalised in 297 cases and
        // lowercase in 89, the opposite of `::X_var` slots. So the source's
        // spelling is kept rather than normalised.
        let ret = self.intern(
            f.return_type
                .as_ref()
                .map(|t| t.0.as_str())
                .unwrap_or("None"),
        );
        let doc = self.intern(f.doc.as_deref().unwrap_or(""));

        self.cur_return = f.return_type.as_ref().map(|t| t.0.clone());
        let mut buf = FnBuf::new();

        // `self` is *not* a local. The locals table holds temporaries and
        // declared `local`s only — the game's `OnActivate` lists six `::tempN`
        // and `::NoneVar`, with neither `self` nor its parameter. Measured on
        // DLC2ThirskDoorScript.
        let _self_idx = self.intern("self");

        let mut params = Vec::new();
        // A parameter the body never reads **still consumes two temporary
        // numbers**. Measured across five functions that differ only in their
        // parameter list and body order:
        //
        //   Function Zero()                    -> first temp ::temp0
        //   Function One(int n)   (n unused)   -> first temp ::temp2
        //   Function Two(int n, int k)         -> first temp ::temp4
        //   Function Three(int n, int k, int j)-> first temp ::temp6
        //   Function ReadsParam(int n) { int m = n ... } -> ::temp0 (n is read)
        //
        // So it is two slots per **unread** parameter, not one per parameter,
        // and a read parameter consumes none. The slots are never declared in
        // the locals table, which is why the numbers look like they skip.
        for p in &f.params {
            let pn = self.intern(&p.name);
            let pt = self.intern(&p.type_name.0);
            buf.types.push((pn, pt));
            buf.params.push((p.name.to_ascii_lowercase(), pn));
            // NOTE: parameters do seem to consume temporary *numbers* — a
            // function with one unused parameter starts at `::temp2` and one
            // with two at `::temp4` — but the rule is not `2 * params` and not
            // `2 * (index of the first read parameter)` either: both fit the
            // hand-built probes and both cost corpus-wide (`semcensus` reads
            // 0.216 instead of 0.220). Left unmodelled rather than fitted,
            // because a wrong offset shifts every temporary in the function.
            params.push(crate::pex::TypedName {
                name: pn,
                type_name: pt,
            });
        }
        for l in &f.locals {
            let ln = self.intern(&l.name);
            let lt = self.var_type(&l.type_name.0);
            buf.locals.push((ln, lt));
            buf.types.push((ln, lt));
            buf.declared.push((l.name.to_ascii_lowercase(), ln));
        }

        self.block(&mut buf, &f.body)?;

        // There is **no implicit trailing `return`**. Measured with the real
        // compiler on six bodies — empty, a plain assignment, an `if`, a
        // `while`, a `None`-returning call, and an `if` whose body ends in one:
        // not one of them ends in a `return`. A void function simply stops.
        //
        // The VM loads and runs these, so the earlier habit of appending
        // `return ::NoneVar` was adding an instruction the game never has, in
        // every function that did not end with an explicit `return`.

        let lines = buf.lines.clone();
        Ok((
            Function {
                name,
                return_type: ret,
                doc,
                user_flags: 0,
                // Bit 0 is `global`, bit 1 is `native`. A native function has
                // no body in the `.pex` — its code lives in the engine — so
                // emitting flag 0 made the VM look for instructions that are
                // not there. `activemagiceffect` declares dozens of these and
                // the corpus gives every one of them flags=2.
                flags: (if f.is_global { 0x1 } else { 0 }) | (if f.is_native { 0x2 } else { 0 }),
                params,
                locals: buf
                    .locals
                    .iter()
                    .map(|(n, t)| crate::pex::TypedName {
                        name: *n,
                        type_name: *t,
                    })
                    .collect(),
                instructions: buf.instructions,
            },
            lines,
        ))
    }

    fn none_var(&mut self, buf: &mut FnBuf) -> StrIndex {
        if let Some(i) = buf.none_var {
            return i;
        }
        let i = self.intern("::NoneVar");
        buf.none_var = Some(i);
        let none_type = self.intern("None");
        buf.locals.push((i, none_type));
        buf.types.push((i, none_type));
        i
    }

    /// A fresh temporary of the given type.
    ///
    /// The type is not cosmetic: `::temp0` is `Int` in a function that adds
    /// integers and `Float` in one that adds floats, and which arithmetic
    /// opcode is emitted depends on it.
    fn temp(&mut self, buf: &mut FnBuf, ty: &str) -> StrIndex {
        // A `None` result is not a temporary: the CK writes it to `::NoneVar`,
        // which exists once per function. `Debug.Trace(...)` as a statement
        // compiles to `callstatic debug Trace ::NoneVar`, and allocating a
        // fresh `::tempN` there both adds a local and makes every later
        // temporary in the function one number too high.
        if ty.eq_ignore_ascii_case("none") {
            return self.none_var(buf);
        }
        // Reuse a released temporary of the same type before allocating a new
        // one — the Creation Kit's allocator is a free list keyed by type, and
        // a different type needs a different slot because the type is part of
        // the declaration the file records.
        // FIFO within a type: the free list is keyed by type name and the
        // **earliest-released** slot of that type is handed back first, not the
        // most recent one. The list itself is per function; the counter below is
        // script-wide and never resets, which is why the two behave differently.
        //
        // `ArenaCrowdQuestScript.MoveEveryoneIntoPlace` still alternates
        // `::temp0 ::temp1` from one statement to the next — that falls out of
        // the ORDER the slots are released in, not out of the list being a stack.
        let t = self.var_type(ty);
        if let Some(at) = buf.free_temps.iter().position(|(_, ft)| *ft == t) {
            let (i, _) = buf.free_temps.remove(at);
            return i;
        }
        let n = self.next_temp;
        self.next_temp += 1;
        let i = self.intern(&format!("::temp{n}"));
        buf.locals.push((i, t));
        buf.types.push((i, t));
        i
    }

    /// Release a temporary once the instruction that consumed it is emitted.
    ///
    /// Only a real temporary is releasable: a parameter, a property's backing
    /// variable or a source `local` is a name the script owns, and freeing one
    /// would let a later expression overwrite it.
    fn release_temp(&mut self, buf: &mut FnBuf, v: &Value) {
        let Value::Identifier(i) = v else { return };
        let is_temp = self
            .strings
            .get(*i as usize)
            .map(|s| s.starts_with("::temp"))
            .unwrap_or(false);
        if !is_temp {
            return;
        }
        // A slot can be **free once**. Releasing the same temporary twice is not
        // a numbering curiosity: two entries for one slot let the allocator hand
        // it out while the first user still holds a value in it, which is a
        // wrong program rather than a different one. The comparison path did
        // exactly that — it released the cast's result slot and then released
        // the same slot again from its operands list.
        if buf.free_temps.iter().any(|(f, _)| f == i) {
            return;
        }
        if let Some((_, t)) = buf.locals.iter().find(|(n, _)| n == i) {
            buf.free_temps.push((*i, *t));
        }
    }

    fn block(&mut self, buf: &mut FnBuf, body: &[Stmt]) -> Result<(), Error> {
        for s in body {
            self.stmt(buf, s)?;
        }
        Ok(())
    }

    fn stmt(&mut self, buf: &mut FnBuf, s: &Stmt) -> Result<(), Error> {
        match s {
            Stmt::Local(l) => {
                let ln = self.intern(&l.name);
                let lt = self.var_type(&l.type_name.0);
                // The function's locals were already declared from the
                // declaration list the parser collected, so declaring again here
                // put the name in the locals table **twice**: `Actor ref = ...`
                // in `ArenaTransitionCombatQuest.Cleanup` gave
                //   game  ref:actor
                //   mine  ref:Actor, ref:Actor
                // A duplicated slot is not just a byte difference — the type
                // multiset is what every later decision reads.
                if !buf.locals.iter().any(|(n, _)| *n == ln) {
                    buf.locals.push((ln, lt));
                    buf.types.push((ln, lt));
                    buf.declared.push((l.name.to_ascii_lowercase(), ln));
                }
                // A declaration with an initial value is an assignment, and —
                // like any assignment — it releases the temporary it consumed.
                // Leaving that out meant `int d = a + b` held `::temp0` for the
                // rest of the function, so the next sum took `::temp1` where the
                // CK reuses `::temp0`. Measured on `Tn.psc`.
                if let Some(init) = &l.initial {
                    let dest = Value::Identifier(ln);
                    let raw = self.expr(buf, init, l.line)?;
                    // A local's initial value is converted to the local's
                    // declared type, exactly like a plain assignment. Measured
                    // with the real compiler on `Form f = akProp` where akProp
                    // is an `Actor`:
                    //   cast ::temp0 ::akProp_var
                    //   assign f ::temp0
                    // `Actor a = Game.GetPlayer()` has NO cast, because the call
                    // already yields an actor — so the rule is "types differ",
                    // not "always cast".
                    let v = self.coerce_arg(buf, raw, &l.type_name.0, l.line);
                    let mut ins = Instruction::new(opcode_of("assign"));
                    ins.args.push(dest);
                    ins.args.push(v.clone());
                    buf.push(ins, l.line);
                    self.release_temp(buf, &v);
                }
                Ok(())
            }
            Stmt::Assign {
                target,
                value,
                line,
            } => {
                // `arr[i] = v` is not an `assign` at all — it is
                // `array_setelement arr i v`, and the value being stored is
                // materialised into a temporary first because that is what the
                // instruction takes. Measured on `dlc1referencealiasarrayscript`
                // (`RefArray[0] = RefToAdd` is `array_setelement ::RefArray_var
                // 0 ::temp3`) and `byohrelationshipadoptableaccessor`.
                if let Expr::Index(arr, idx) = target {
                    let a = self.expr(buf, arr, *line)?;
                    let i = self.expr(buf, idx, *line)?;
                    let v = self.expr(buf, value, *line)?;
                    let ty = self.value_type(buf, &v);
                    let v = self.materialise(buf, v, &ty, *line);
                    let mut ins = Instruction::new(opcode_of("array_setelement"));
                    ins.args.push(a);
                    ins.args.push(i);
                    ins.args.push(v);
                    buf.push(ins, *line);
                    return Ok(());
                }
                // `a.b = v` is **not** an `assign`: it is `propset b a v`, with
                // the property name first. Falling through to the plain assign
                // path emitted `assign ::PropName v`, which is not an
                // instruction the VM can execute. Measured over all 3,352
                // `propset` instructions in the shipped corpus: slot 2 is a
                // temporary every time (the value), slot 1 is a temporary only
                // 964 times (the object), and slot 0 never is (the property
                // name). `examples/propargs.rs` counts it.
                if let Expr::Member(obj, name) = target {
                    // The VALUE is evaluated before the object: `propset` is
                    // `name, object, value`, and the operands go right to left
                    // like a call's. Measured on
                    // `BYOH_SF_RelationshipAdoption__0101652D.Fragment_0`, where
                    // `(GetOwningQuest() as …).OrderToConfirm = 1.3` emits
                    //   assign ::temp1 1.3
                    //   callmethod GetOwningQuest self ::temp2
                    //   cast ::temp3 ::temp2
                    //   propset OrderToConfirm ::temp3 ::temp1
                    // — the literal is materialised BEFORE the receiver is
                    // evaluated, not after.
                    let p = self.intern(name);
                    let raw = self.expr(buf, value, *line)?;
                    // The VALUE is materialised into a temporary, always: slot 2
                    // of all 3,352 shipped `propset` instructions is a `::tempN`
                    // and never a literal. `kmyQuest.SomeProp = true` compiles to
                    //   assign ::temp17 true
                    //   propset SomeProp kmyQuest ::temp17
                    // and the CK reuses that one temp for consecutive propsets,
                    // so releasing it after the instruction is what makes the
                    // next one take the same slot.
                    let ty = self.value_type(buf, &raw);
                    let v = self.materialise(buf, raw, &ty, *line);
                    let o = self.expr(buf, obj, *line)?;
                    let mut ins = Instruction::new(opcode_of("propset"));
                    ins.args.push(Value::Identifier(p));
                    ins.args.push(o);
                    ins.args.push(v.clone());
                    buf.push(ins, *line);
                    // The materialised value is dead once the property is
                    // written, and the CK reuses that one slot for consecutive
                    // propsets (`assign ::temp17 true` … `assign ::temp17 true`).
                    self.release_temp(buf, &v);
                    return Ok(());
                }
                let want = self.target_type(buf, target);
                let dest = self.lvalue(buf, target)?;
                let raw = self.expr(buf, value, *line)?;
                // A value entering a typed slot is converted to that slot's
                // type, not only at call arguments. Measured with the real
                // compiler:
                //   Actor a = Game.GetPlayer()   -> no cast, ::temp0 is already actor
                //   Form f = akProp (akProp: Actor)
                //     -> cast ::temp0 ::akProp_var ; assign f ::temp0
                // Same rule, same target type: the destination's.
                let v = match &want {
                    Some(w) => self.coerce_arg(buf, raw, w, *line),
                    None => raw,
                };
                let mut ins = Instruction::new(opcode_of("assign"));
                ins.args.push(dest);
                ins.args.push(v.clone());
                buf.push(ins, *line);
                self.release_temp(buf, &v);
                Ok(())
            }
            Stmt::CompoundAssign {
                target,
                op,
                value,
                line,
            } => {
                // `x += 1` is `x = x + 1` and the compiler emits exactly that:
                // a read, an arithmetic instruction, and an assign.
                let dest = self.lvalue(buf, target)?;
                let rhs = self.expr(buf, value, *line)?;
                let dt = self.value_type(buf, &dest);
                let tmp = self.temp(buf, &dt);
                let arith = self.arith_op(buf, op, &dest);
                let mut ins = Instruction::new(arith);
                ins.args.push(Value::Identifier(tmp));
                ins.args.push(dest.clone());
                ins.args.push(rhs);
                buf.push(ins, *line);
                let mut ins = Instruction::new(opcode_of("assign"));
                ins.args.push(dest);
                ins.args.push(Value::Identifier(tmp));
                buf.push(ins, *line);
                Ok(())
            }
            Stmt::Expr(e, line) => {
                let v = self.expr(buf, e, *line)?;
                self.release_temp(buf, &v);
                Ok(())
            }
            Stmt::Return { value, line } => {
                // A bare `return` returns the **None literal**, not the
                // `::NoneVar` identifier: the game writes `return None` (value
                // tag 0). The expression path turns source `None` into
                // `::NoneVar`, which is the right identifier for a call
                // destination but the wrong operand for a return.
                let raw = if matches!(value, Expr::None) {
                    Value::None
                } else {
                    self.expr(buf, value, *line)?
                };
                // The returned value is converted to the function's declared
                // return type, exactly like an assignment target. Measured on
                // Ret1/Ret2/Ret3 with the real compiler.
                let v = match self.cur_return.clone() {
                    Some(w) => self.coerce_arg(buf, raw, &w, *line),
                    None => raw,
                };
                let mut ins = Instruction::new(opcode_of("return"));
                ins.args.push(v.clone());
                buf.push(ins, *line);
                self.release_temp(buf, &v);
                Ok(())
            }
            Stmt::Break { line } => {
                let mut ins = Instruction::new(opcode_of("jmp"));
                ins.args.push(Value::Int(0));
                buf.push(ins, *line);
                Ok(())
            }
            Stmt::If { branches, line } => self.if_stmt(buf, branches, *line),
            Stmt::While { cond, body, line } => {
                // `while (c) body endWhile` is:
                //   start: cond -> temp ; jmpf temp <exit-start>
                //          body
                //          jmp <start - here>
                let start = buf.here();
                let c = self.expr(buf, cond, *line)?;
                let mut ins = Instruction::new(opcode_of("jmpf"));
                ins.args.push(c);
                let at = buf.here();
                ins.args.push(Value::Int(0));
                buf.push(ins, *line);
                self.block(buf, body)?;
                // The backward jump is relative to ITS OWN index, not to the
                // `jmpf`'s. Using `at` made every loop jump one instruction too
                // far forward — `while` compiled to `jmp -1` where the CK has
                // `jmp -4`.
                let here = buf.here();
                let mut ins = Instruction::new(opcode_of("jmp"));
                ins.args.push(Value::Int(start as i32 - here as i32));
                buf.push(ins, *line);
                let after = buf.here();
                if let Value::Int(v) = &mut buf.instructions[at].args[1] {
                    *v = after as i32 - at as i32;
                }
                Ok(())
            }
        }
    }

    fn if_stmt(&mut self, buf: &mut FnBuf, branches: &[Branch], line: u16) -> Result<(), Error> {
        // Each branch whose condition is false jumps to the next branch's
        // start; each branch's body ends with a jump past the whole chain.
        let mut patch_false: Vec<usize> = Vec::new();
        let mut patch_end: Vec<usize> = Vec::new();
        // A branch's `jmpf` targets the START OF THE NEXT BRANCH, not the end of
        // the chain. In `if/elseif/elseif/elseif` the game emits
        // `cmp_eq, jmpf +3, call, jmp +13` for every branch: false falls through
        // to the next condition three instructions later, while a taken branch
        // jumps clear of them all. Pointing `jmpf` at the chain end as well made
        // a false condition skip the remaining branches entirely — the same
        // instruction count and completely different behaviour.
        let mut pending_false: Option<usize> = None;
        self.if_depth += 1;
        let mark = self.cond_temps.len();
        // The first branch's condition temp, held until the construct ends.
        let mut first_cond: Option<StrIndex> = None;

        for (mine_branch, b) in branches.iter().enumerate() {
            // We are at the start of this branch, so the previous branch's
            // failed condition lands here. It has to be AFTER that branch's
            // trailing `jmp`, which is why this is not done at the end of the
            // loop body: landing before the `jmp` was one instruction short.
            if let Some(prev) = pending_false.take() {
                let here = buf.here();
                if let Value::Int(v) = &mut buf.instructions[prev].args[1] {
                    *v = here as i32 - prev as i32;
                }
            }
            let mut mine_cond: Option<StrIndex> = None;
            if let Some(cond) = &b.cond {
                let c = self.expr(buf, cond, b.line)?;
                let mut ins = Instruction::new(opcode_of("jmpf"));
                ins.args.push(c.clone());
                let at = buf.here();
                ins.args.push(Value::Int(0));
                buf.push(ins, b.line);
                // NOT released here. See the `cond_temps` field: the CK holds
                // every condition temp of the construct until the outermost `if`
                // finishes, then frees them in allocation order. Freeing at the
                // `jmpf` would hand the second branch of an `elseif` the first
                // branch's slot, which the real compiler does not do.
                if let Value::Identifier(ci) = c {
                    self.cond_temps.push(ci);
                    mine_cond = Some(ci);
                    if first_cond.is_none() {
                        first_cond = Some(ci);
                    }
                }
                pending_false = Some(at);
                patch_false.push(at);
            }
            self.block(buf, &b.body)?;
            // Every branch body ends with a jump past the rest of the chain,
            // except that an **empty** branch in a multi-branch construct gets
            // none. Measured with the real compiler on `Actor.MakePlayerFriend`,
            // whose inner and outer `if`s both have a comment-only `else`:
            //
            //   10 jmp 1     <- inner then-branch
            //   11 jmp 1     <- outer then-branch, and that is the end
            //
            // Both empty `else` bodies contribute nothing. A single-branch
            // `if` still emits its jump even when the body is empty
            // (`if c endif` -> `jmpf c 2 ; jmp 1`, Em3), which is why the count
            // is not simply `bodies.len()`.
            // A branch whose body ends in `return` gets no trailing jump: the
            // jump would be unreachable. Measured on
            // `ArenaCombatQuest.ShouldEnd`, whose `if/else` both `return`:
            //
            //   2 return true
            //   3 jmp 2          <- the THEN branch keeps its jump
            //   4 return false   <- the ELSE branch has none
            //
            // which is the same dead-code elimination that leaves an empty
            // branch with no jump at all.
            // Only the LAST branch drops it, and only when it ends in `return`:
            // in `ArenaCombatQuest.ShouldEnd` the then-branch keeps its jump and
            // the else-branch has none. Applying the rule to every branch scores
            // 43,624 against 43,639, so earlier branches keep theirs.
            let is_last = mine_branch + 1 == branches.len();
            // ...and only for a MULTI-branch construct. A single-branch
            // `if c return endif` DOES get its trailing jump: the false path has
            // to skip the return, and the game emits
            //   jmpf ::temp0 3 / return None / jmp 1
            // Measured on `CWSoldierDeathObserverActorScript.OnDeath`.
            let last_and_returns =
                is_last && branches.len() > 1 && matches!(b.body.last(), Some(Stmt::Return { .. }));
            // An EMPTY branch drops its jump only when it is the LAST branch.
            // `ActivateAgainAfterDelay.onActivate` has a comment-only `if` body
            // followed by a real `else`, and the game still emits the then
            // branch's `jmp 17` — dropping it makes the false path fall into the
            // else correctly but the true path skip nothing. A single-branch
            // `if` keeps its jump even when empty (`if c endif`).
            let last_and_empty = is_last && b.body.is_empty() && branches.len() > 1;
            // NOTE: a tail-position rule was also tried — a nested `if` as the
            // final statement of an enclosing branch skipping its last jump,
            // because `ArenaInteriorDoorScript.OnActivate` emits one `jmp` where
            // this emits two. It scores 5,275 behavioural differences against
            // 4,602, so the elision is conditional on something not yet
            // identified and the rule is left out.
            let skip = last_and_empty || last_and_returns;
            if !skip {
                let mut ins = Instruction::new(opcode_of("jmp"));
                ins.args.push(Value::Int(0));
                let at = buf.here();
                buf.push(ins, b.line);
                patch_end.push(at);
            }
            // The CK frees a branch's condition temp at the end of that branch,
            // so `elseif` reuses one slot for every branch after the first.
            // Measured: `if/elseif/elseif/elseif` gets ::temp0 then ::temp1
            // three times over — branch 1's ::temp0 is NOT recycled.
            if mine_branch > 0 {
                if let Some(ci) = mine_cond {
                    self.release_temp(buf, &Value::Identifier(ci));
                }
            }
        }
        // The first branch's condition temp is released here, at the end of the
        // whole construct. Restricting this to single-branch `if`s was also
        // tried, because `if/elseif/…` really does hand a following `if` the
        // ::temp1 slot rather than ::temp0 — but it scores 41,210 identical
        // functions against 41,230 for releasing unconditionally, so the reuse
        // rule for a chain's first condition is still not decoded. Recorded
        // rather than guessed at any further.
        if let Some(ci) = first_cond {
            self.release_temp(buf, &Value::Identifier(ci));
        }
        self.cond_temps.truncate(mark);

        self.if_depth -= 1;
        let end = buf.here();
        if let Some(prev) = pending_false.take() {
            if let Value::Int(v) = &mut buf.instructions[prev].args[1] {
                *v = end as i32 - prev as i32;
            }
        }
        // The LAST branch's failed condition falls through to the end of the
        // chain, which is here. Every earlier one was already pointed at the
        // start of the branch that follows it.
        for at in patch_false {
            if let Value::Int(v) = &mut buf.instructions[at].args[1] {
                if *v == 0 {
                    *v = end as i32 - at as i32;
                }
            }
        }
        for at in patch_end {
            if let Value::Int(v) = &mut buf.instructions[at].args[0] {
                *v = end as i32 - at as i32;
            }
        }
        let _ = line;
        Ok(())
    }

    /// Put a value into a temporary if the instruction cannot take it directly.
    ///
    /// The compiler does this for every operand of `array_setelement`: a
    /// literal or a call result is stored into a `::tempN` and the instruction
    /// takes that identifier. Measured on `dlc1vampireshrinescript`, where
    /// `itemValues[myValue] = true` compiles to `assign ::temp8 bool` followed
    /// by `array_setelement itemValues myValue ::temp8`.
    fn materialise(&mut self, buf: &mut FnBuf, v: Value, ty: &str, line: u16) -> Value {
        // **Already a temporary** passes through; anything else is copied into
        // one — including a plain variable or parameter. The rule is that slot 2
        // of a `propset` is always a `::tempN`, not merely always non-literal:
        //   `DA02.stage = stageToSet`      (stageToSet is an int variable)
        //   -> assign ::temp0 stageToSet
        //      propset stage ::DA02_var ::temp0
        // Returning an identifier unchanged covered literals and call results but
        // left variables bare, which cost the temporary AND started the
        // script-wide counter one lower than the game's, shifting every later
        // function in the file. Witness: DA02FixStageScript.
        let already_temp = match &v {
            Value::Identifier(i) => self
                .strings
                .get(*i as usize)
                .map(|s| s.starts_with("::temp"))
                .unwrap_or(false),
            _ => false,
        };
        if already_temp {
            return v;
        }
        let tmp = self.temp(buf, ty);
        let mut ins = Instruction::new(opcode_of("assign"));
        ins.args.push(Value::Identifier(tmp));
        ins.args.push(v);
        buf.push(ins, line);
        Value::Identifier(tmp)
    }

    /// A value that can be assigned to.
    /// The declared type of an assignment target, when the compiler knows it.
    ///
    /// Used to decide whether the value needs a `cast` on the way in. A local or
    /// parameter is in the function's type table; a property is a script-level
    /// declaration, which `type_of` covers. Anything else (`arr[i]`) has no
    /// single declared type and returns `None`, leaving the value untouched.
    fn target_type(&self, buf: &FnBuf, e: &Expr) -> Option<String> {
        match e {
            Expr::Ident(n) => {
                let lower = n.to_ascii_lowercase();
                if let Some((_, i)) = buf
                    .params
                    .iter()
                    .chain(&buf.declared)
                    .find(|(p, _)| p == &lower)
                {
                    return self.declared_type(buf, *i);
                }
                self.type_of(n).map(|t| t.0.clone())
            }
            Expr::Member(_, name) => Some(self.property_type(name).to_string()),
            _ => None,
        }
    }

    fn lvalue(&mut self, buf: &mut FnBuf, e: &Expr) -> Result<Value, Error> {
        match e {
            Expr::Ident(n) => {
                let lower = n.to_ascii_lowercase();
                if let Some((_, i)) = buf
                    .params
                    .iter()
                    .chain(&buf.declared)
                    .find(|(p, _)| p == &lower)
                {
                    return Ok(Value::Identifier(*i));
                }
                // A script-level name. A property's storage is its backing
                // variable, `::Name_var`: the shipped corpus writes
                // `assign ::HardWorkerChoppedWood_var true` for
                // `HardWorkerChoppedWood = true`, never the bare name. An
                // AutoReadOnly property has no backing variable — its value is a
                // constant folded into the getter — so it is left alone.
                if self.is_auto_property(n) {
                    let backing = format!("::{n}_var");
                    return Ok(Value::Identifier(self.intern(&backing)));
                }
                Ok(Value::Identifier(self.intern(n)))
            }
            Expr::Member(obj, name) => {
                // `a.b = v` is `propset a b v`; the object is a value.
                let _ = self.expr(buf, obj, 0)?;
                // An identifier, like `propget`: all 3,352 propset names in
                // the shipped corpus carry tag byte 1.
                Ok(Value::Identifier(self.intern(name)))
            }
            other => Err(Error::Parse(format!("not assignable: {other:?}"))),
        }
    }

    /// The arithmetic opcode for `op` over `lhs`.
    ///
    /// An integer expression compiles to `iadd`/`isub`/`imul`/`idiv` and a
    /// float one to the `f` forms. The operand's own type is what decides, so
    /// this reads the type of the value it was actually given.
    fn arith_op(&self, buf: &FnBuf, op: &str, lhs: &Value) -> u8 {
        // The operand's type comes from the *function's* type environment, not
        // the compiler's: `int a = 1` declares `a` in the buffer, so asking the
        // script-level table found nothing and every integer sum compiled to
        // `fadd`. Measured on `Temps.psc`, where the CK emits `iadd`.
        // Compared case-insensitively: a local's declared type is interned as
        // the source spelled it (`int`), while the canonical slot spelling is
        // `Int`. An exact compare made every integer sum a float one.
        let int = self.value_type(buf, lhs).eq_ignore_ascii_case("int");
        match (op, int) {
            ("+", true) => opcode_of("iadd"),
            ("+", false) => opcode_of("fadd"),
            ("-", true) => opcode_of("isub"),
            ("-", false) => opcode_of("fsub"),
            ("*", true) => opcode_of("imul"),
            ("*", false) => opcode_of("fmul"),
            ("/", true) => opcode_of("idiv"),
            ("/", false) => opcode_of("fdiv"),
            ("%", _) => opcode_of("imod"),
            _ => opcode_of("fadd"),
        }
    }

    /// The type a call's result temporary carries.
    ///
    /// The engine's own functions are declared in the game's script sources,
    /// which a single-file compile does not read, so the type is `var` — the
    /// universal type the VM accepts — unless the name is one of the few whose
    /// result type the compiled bytes themselves show. `GetState` returns a
    /// `String` in every shipped script that has it.
    fn call_result_type(&self, name: &str) -> &'static str {
        // The DECLARATIONS first: `Actor Function F()` states its result type
        // outright, and 30,148 names carry one. That is direct evidence, unlike
        // `crate::returns`, which infers a result type from the temporary the
        // call was stored into and therefore only knows calls that happen to
        // appear in a shipped script.
        //
        // Falling back to `var` is what made `EffectShader.Play(...)` allocate a
        // `var` temporary as its destination: `Play` returns None, so the game
        // writes `::NoneVar` and allocates nothing. A wrong result type thus
        // costs a local AND shifts every later temporary in the script, because
        // the counter is script-wide.
        // Order matters and was measured: the LOCALS-derived table must win.
        // Putting the declaration table first scores 41,137 against 42,479 — the
        // declarations describe the class as its source defines it, while the
        // shipped `.pex` was built against whatever the game actually had, and
        // the two disagree for long enough to matter. So declarations only fill
        // in names the locals table never saw.
        if let Some(t) = crate::returns::method_return(name) {
            return t;
        }
        let own = self.own_class().unwrap_or_default();
        if let Some(t) = crate::signatures::return_type(&own, name) {
            return t;
        }
        match name.to_ascii_lowercase().as_str() {
            "getstate" => "String",
            "getvalueint" | "getvalue" => "Int",
            "isactivationblocked" | "isplaying" | "isrunning" => "Bool",
            _ => "var",
        }
    }

    /// What `class.function()` returns, for a call whose result has to be
    /// given a type before the instruction that will hold it is chosen.
    ///
    /// The answer comes from [`crate::returns`], which is measured from the
    /// shipped scripts rather than transcribed — a compile unit cannot see the
    /// class's own source, so this is the only way to know.
    fn static_result_type(&self, class: &str, function: &str) -> &'static str {
        // NOTE: also tried consulting the declaration-derived table here, both
        // class-qualified and class-agnostic. Both score worse — the
        // class-agnostic fallback takes behavioural differences from 4,556 to
        // 5,059 — because a name shared across unrelated classes then resolves
        // to one arbitrary type. The locals-derived table stays authoritative.
        crate::returns::static_return(class, function).unwrap_or("var")
    }

    /// A type name as a **variable slot** spells it.
    ///
    /// Measured over every `::X_var` in the corpus: a primitive is always
    /// capitalised (`Int` 2,718, `Bool` 1,552, `Float` 766, `String` 169 — no
    /// lowercase spelling at all), while a class name is lowercase in 91% of
    /// the 37,235 cases. The source spells these however it likes, so the slot
    /// is normalised rather than copied.
    fn var_type(&mut self, name: &str) -> StrIndex {
        let lower = name
            .trim()
            .trim_end_matches("[]")
            .trim()
            .to_ascii_lowercase();
        let canonical = match lower.as_str() {
            "int" => "Int".to_string(),
            "float" => "Float".to_string(),
            "bool" => "Bool".to_string(),
            "string" => "String".to_string(),
            "none" => "None".to_string(),
            _ => {
                if name.ends_with("[]") {
                    format!("{lower}[]")
                } else {
                    lower
                }
            }
        };
        self.intern(&canonical)
    }

    /// The `::Name_var` member that stores an `auto` property, if `name` is one.
    fn auto_backing_var(&self, name: &str) -> Option<String> {
        self.script.decls.iter().find_map(|d| match &d.kind {
            DeclKind::Property(p) if p.is_auto && p.name.eq_ignore_ascii_case(name) => {
                Some(format!("::{}_var", p.name))
            }
            _ => None,
        })
    }

    /// The type this script `extends`, which is the class a bare `f()` call
    /// implicitly dispatches through.
    fn own_class(&self) -> Option<String> {
        self.script.parent.clone()
    }

    /// The declared type of a property, for its `propget` temporary.
    fn property_type(&self, name: &str) -> &'static str {
        match self.type_of(name) {
            Some(t) => match t.base().as_str() {
                "int" => "Int",
                "float" => "Float",
                "bool" => "Bool",
                "string" => "String",
                _ => "var",
            },
            None => "var",
        }
    }

    /// `a && b` and `a || b`, which short-circuit.
    ///
    /// The VM has no boolean-and instruction: both operators are a jump and an
    /// assignment, and the shape is the one the shipped corpus uses.
    ///
    /// ```text
    /// a && b          a || b
    ///   t1 = a          t1 = a
    ///   jmpf t1 L       jmpt t1 L
    ///   t1 = b          t1 = b
    /// L:                L:
    /// ```
    ///
    /// The first operand's value is assigned into the result slot *before* the
    /// jump, so a short-circuit leaves the result holding the operand that
    /// decided it — `false && x` is `false` and `true || x` is `true`, which is
    /// what the language means.
    fn logical(
        &mut self,
        buf: &mut FnBuf,
        op: &str,
        lhs: &Expr,
        rhs: &Expr,
        line: u16,
    ) -> Result<Value, Error> {
        // The left operand is evaluated FIRST, then released, and only then is
        // the result slot allocated — so it lands on the operand's own slot and
        // the cast becomes a cast of a temporary to itself:
        //   cmp_eq ::temp2 akSource ::temp1
        //   cast ::temp2 ::temp2
        // Allocating the result first put it on a fresh slot instead
        // (`cast ::temp0 ::temp3`) and cost an extra Bool local in 1,804
        // functions. Measured on `AltarOfMolagBalScript.OnAnimationEvent`.
        let a = self.expr(buf, lhs, line)?;
        self.release_temp(buf, &a);
        let result = self.temp(buf, "Bool");
        // The operand is **cast** into the result slot, not assigned. Measured
        // on `AudioRepeaterActivator01Script.OnCellAttach`, whose
        // `while (bRunning && is3DLoaded())` compiles to
        //   cast ::temp0 bRunning
        //   jmpf ::temp0 …
        // and then `cast ::temp0 ::temp0` for the right-hand side, which has
        // already landed in that very slot — a cast of a temporary to itself,
        // which the compiler emits anyway because the destination is fixed.
        let mut ins = Instruction::new(opcode_of("cast"));
        ins.args.push(Value::Identifier(result));
        ins.args.push(a);
        buf.push(ins, line);

        // `&&` jumps when the left side is false; `||` when it is true.
        let mut ins = Instruction::new(opcode_of(if op == "&&" { "jmpf" } else { "jmpt" }));
        ins.args.push(Value::Identifier(result));
        let at = buf.here();
        ins.args.push(Value::Int(0));
        buf.push(ins, line);

        let b = self.expr(buf, rhs, line)?;
        let mut ins = Instruction::new(opcode_of("cast"));
        ins.args.push(Value::Identifier(result));
        ins.args.push(b);
        buf.push(ins, line);

        let end = buf.here();
        if let Value::Int(v) = &mut buf.instructions[at].args[1] {
            *v = end as i32 - at as i32;
        }
        Ok(Value::Identifier(result))
    }

    /// Whether a compile-time value is an integer.
    fn value_is_int(&self, v: &Value) -> bool {
        match v {
            Value::Int(_) => true,
            Value::Float(_) => false,
            Value::Identifier(i) => match self.strings.get(*i as usize) {
                Some(name) => self.is_int(name),
                None => false,
            },
            _ => false,
        }
    }

    /// Append the default value of every trailing parameter the source omitted.
    ///
    /// The compiler **writes out a literal for each omitted argument**, so a
    /// call written short in source is longer in the `.pex`:
    ///
    /// ```text
    /// source:  BlockActivation()      ->  callmethod blockActivation self ::NoneVar true
    /// source:  Activate(akActionRef)  ->  callmethod activate ::temp0 ::temp1 actronaut false
    /// source:  GetLinkedRef()         ->  callmethod getLinkedRef self ::temp0 None
    /// ```
    ///
    /// Without this the call is emitted short and the loader sees a different
    /// argument count from the one the game shipped.
    ///
    /// A parameter with **no** default is never padded: the source was required
    /// to supply it, and filling in a value there would be invention.
    fn pad_defaults(
        &mut self,
        mut args: Vec<Value>,
        sig: Option<&'static [Option<(&'static str, &'static str)>]>,
        function: &str,
    ) -> Vec<Value> {
        // A function with no usable signature can still be padded from the
        // per-position table. `additem` is the canonical case: its signature is
        // dropped as ambiguous, but every declaration that reaches slots 1 and 2
        // agrees they default to `1` and `false`, and the game writes both out.
        let slots: Vec<Option<(String, String)>> = match sig {
            Some(s) => s
                .iter()
                .map(|o| o.map(|(k, v)| (k.to_string(), v.to_string())))
                .collect(),
            None => {
                let mut v = Vec::new();
                for i in 0..8 {
                    match crate::signatures::positional_default(function, i) {
                        Some(Some((k, val))) => v.push(Some((k.to_string(), val.to_string()))),
                        Some(None) => v.push(None),
                        // Past the end of what the table knows: stop.
                        None => break,
                    }
                }
                v
            }
        };
        if slots.is_empty() {
            return args;
        }
        for slot in slots.iter().skip(args.len()) {
            let Some((kind, text)) = slot else { break };
            let (kind, text) = (kind.as_str(), text.as_str());
            let v = match kind {
                "bool" => Value::Bool(text == "true"),
                "none" => Value::None,
                "int" => match text.parse::<i32>() {
                    Ok(n) => Value::Int(n),
                    Err(_) => break,
                },
                "float" => match text.parse::<f32>() {
                    Ok(f) => Value::Float(f),
                    Err(_) => break,
                },
                "str" => Value::String(self.intern(text)),
                _ => break,
            };
            args.push(v);
        }
        args
    }

    /// The type a call argument has to be converted to, given the parameter
    /// signature for `method`.
    ///
    /// The compiler converts **type-directed**, not by materialising every
    /// identifier. The real compiler shows it directly:
    ///
    /// ```text
    /// Self.MoveTo(oProp, fProp, fProp, iProp)
    ///   -> ::oProp_var ::fProp_var ::fProp_var bare, iProp -> cast ::temp0 ::iProp_var
    /// akA.MoveTo(oProp, afV, afV, afV)
    ///   -> every argument bare
    /// ```
    ///
    /// `MoveTo` wants `(ObjectReference, float, float, float, bool)`. `oProp` and
    /// `fProp` already sat in matching slots; `iProp` was an int where a float is
    /// required, so it got a `cast`.
    ///
    /// Only the numeric widening the VM actually performs is modelled: int to
    /// float. Anything else is left alone, because a wrong `cast` costs an
    /// instruction and changes the temporary numbering for the rest of the
    /// function — far worse than leaving an argument alone.
    fn coerce_arg(&mut self, buf: &mut FnBuf, value: Value, want: &str, line: u16) -> Value {
        let want = want.trim_end_matches("[]");
        // The `None` literal materialises into a typed temporary when the slot
        // wants a class. Measured with the real compiler: `Actor.ClearForcedLandingMarker()`
        // calls `SetForcedLandingMarker(None)`, which compiles to
        //   cast ::temp7 None
        //   callmethod SetForcedLandingMarker self ::NoneVar ::temp7
        // Passing `::NoneVar` straight through is the same value and a different
        // instruction. It is the single largest remaining bucket (~2,290
        // functions), because `None` is how Papyrus spells "unset object".
        // The source writes `None`, which the expression path turns into the
        // `::NoneVar` identifier — but the CK casts from a **tag-0 `None`**, not
        // from that variable:
        //   cast ::temp7 None
        // so both spellings have to be recognised here.
        let is_none = match &value {
            Value::None => true,
            Value::Identifier(i) => self
                .strings
                .get(*i as usize)
                .map(|s| s.eq_ignore_ascii_case("::NoneVar"))
                .unwrap_or(false),
            _ => false,
        };
        if is_none {
            if want.is_empty()
                || want.eq_ignore_ascii_case("none")
                || want.eq_ignore_ascii_case("var")
                || want.eq_ignore_ascii_case("nothing")
            {
                return value;
            }
            let t = self.temp(buf, want);
            let mut ins = Instruction::new(opcode_of("cast"));
            ins.args.push(Value::Identifier(t));
            ins.args.push(Value::None);
            buf.push(ins, line);
            return Value::Identifier(t);
        }
        let have = self.value_type(buf, &value);
        if have.eq_ignore_ascii_case(want) || have.eq_ignore_ascii_case("var") {
            return value;
        }
        // Two conversions are modelled, both measured:
        //
        // 1. Numeric widening — an int in a float slot. `Self.MoveTo(oProp, 1, 1, 1)`
        //    casts all three int literals because `MoveTo` wants floats.
        // 2. **Object upcast** — a subtype in a supertype slot. `addItem` wants a
        //    `Form`, and the argument is an `Armor` property, so the CK emits
        //    `cast ::temp1 ::ArmorBriarHeartEmpty_var` first. Decidable only
        //    with the `extends` chain, which is why it needs `PARENTS`.
        // Whether `have` converts to `want` at all. The rule is not "numeric or
        // subclass" — it is a specific ladder, and the permissive branches are
        // easy to miss:
        //
        //   same type            -> no cast (handled above)
        //   target float         -> source int           (widening only)
        //   target bool          -> ANY source           (truthiness)
        //   target string        -> ANY source           (stringification)
        //   target object, source none     -> yes
        //   target object, source subtype  -> yes, walk the extends chain upward
        //   target int,  source float      -> NO (never narrows)
        //
        // The `bool` and `string` branches are the ones that surprise: any value
        // assigned to a bool or string slot is cast, so `bool b = someInt` and
        // `string s = someForm` both emit a `cast` even though no numeric
        // widening is involved.
        let numeric = |t: &str| t.eq_ignore_ascii_case("int") || t.eq_ignore_ascii_case("float");
        let ok = if want.eq_ignore_ascii_case("bool") || want.eq_ignore_ascii_case("string") {
            true
        } else if numeric(&have) && numeric(want) {
            // Only int -> float widens; float -> int is not a conversion.
            want.eq_ignore_ascii_case("float")
        } else {
            // Walk up from the value's class looking for the wanted type.
            let mut here = crate::signatures::parent(&have).map(str::to_string);
            let mut hops = 0;
            let mut found = false;
            while let Some(c) = here {
                if hops > 12 {
                    break;
                }
                if c.eq_ignore_ascii_case(want) {
                    found = true;
                    break;
                }
                here = crate::signatures::parent(&c).map(str::to_string);
                hops += 1;
            }
            found
        };
        if !ok {
            // A conversion the model does not know: leave the argument alone
            // rather than invent a cast.
            return value;
        }
        let t = self.temp(buf, want);
        let mut ins = Instruction::new(opcode_of("cast"));
        ins.args.push(Value::Identifier(t));
        ins.args.push(value.clone());
        buf.push(ins, line);
        self.release_temp(buf, &value);
        Value::Identifier(t)
    }

    /// The declared class of a name used as a receiver, looking through the
    /// function scope first and the script scope second.
    fn receiver_class(&self, buf: &FnBuf, name: &str) -> Option<String> {
        let lower = name.to_ascii_lowercase();
        if let Some((_, i)) = buf
            .params
            .iter()
            .chain(&buf.declared)
            .find(|(p, _)| p == &lower)
        {
            return self.declared_type(buf, *i);
        }
        self.type_of(name).map(|t| t.0.clone())
    }

    /// Whether `name` is an `auto` property, whose storage is a backing variable.
    ///
    /// `AutoReadOnly` is excluded: it has no variable, because its value is a
    /// constant folded into a generated getter.
    fn is_auto_property(&self, name: &str) -> bool {
        let n = name.to_ascii_lowercase();
        self.script.decls.iter().any(|d| match &d.kind {
            crate::parse::DeclKind::Property(p) => {
                p.is_auto && !p.is_read_only && {
                    let pn = p.name.to_ascii_lowercase();
                    pn == n
                }
            }
            _ => false,
        })
    }

    /// Whether a value of type `have` converts into a slot of type `want`.
    ///
    /// The exact ladder: same type, int to float (one-way), anything to bool or
    /// string, `none` to any object, and a subtype to any of its ancestors.
    fn widens_to(&self, want: &str, have: &str) -> bool {
        if have.eq_ignore_ascii_case(want) {
            return true;
        }
        let is = |t: &str, n: &str| t.eq_ignore_ascii_case(n);
        if is(want, "bool") || is(want, "string") {
            return true;
        }
        if is(want, "float") && is(have, "int") {
            return true;
        }
        if is(have, "none") {
            return true;
        }
        let mut here = Some(have.to_ascii_lowercase());
        let mut hops = 0;
        while let Some(c) = here {
            if hops > 12 {
                break;
            }
            if c.eq_ignore_ascii_case(want) {
                return true;
            }
            here = crate::signatures::parent(&c).map(str::to_string);
            hops += 1;
        }
        false
    }

    /// The type name a compiled value carries, for sizing a temporary.
    fn value_type(&self, buf: &FnBuf, v: &Value) -> String {
        match v {
            Value::Float(_) => "Float".to_string(),
            Value::Int(_) => "Int".to_string(),
            Value::Bool(_) => "Bool".to_string(),
            Value::String(_) => "String".to_string(),
            Value::Identifier(i) => {
                // `self` has a type too: the script's own class. Leaving it as
                // `var` meant `self` was never converted, so
                // `DoCombatSpellApply(spell, self)` passed it bare where the CK
                // emits `cast ::temp1 self` (self: actronaut -> ObjectReference).
                // That was the single largest remaining shape difference.
                if self
                    .strings
                    .get(*i as usize)
                    .map(|s| s.eq_ignore_ascii_case("self"))
                    .unwrap_or(false)
                {
                    // The script's OWN class, not its parent. `own_class()`
                    // returns the parent (it is used for static result-type
                    // lookups), and using it here said `self` was already an
                    // `ObjectReference` — so no upcast fired where the CK emits
                    // `cast ::temp1 self` for a parameter declared
                    // `ObjectReference`. A script's `self` is of its own type.
                    return self.script.name.clone();
                }
                // A temporary's type was decided when it was allocated, and
                // that is what a comparison has to see: `akActivator` is an
                // `ObjectReference` and `::temp1` holding `Game.GetPlayer()` is
                // an `actor`, which is exactly what makes the CK emit a cast.
                self.declared_type(buf, *i).unwrap_or_else(|| {
                    if self.value_is_int(v) {
                        "Int".to_string()
                    } else {
                        "Float".to_string()
                    }
                })
            }
            _ => {
                if self.value_is_int(v) {
                    "Int".to_string()
                } else {
                    "Float".to_string()
                }
            }
        }
    }

    /// The type a local, temporary or parameter holds, by its string index.
    ///
    /// The function's own locals table is the authority: `temp()` records the
    /// type it chose, and a `local` declaration records its declared type. This
    /// is what lets a comparison see that its two sides are different classes
    /// and emit the `cast` the Creation Kit emits.
    fn declared_type(&self, buf: &FnBuf, index: StrIndex) -> Option<String> {
        if let Some(t) = buf.types.iter().rev().find(|(n, _)| *n == index) {
            return Some(
                self.strings
                    .get(t.1 as usize)
                    .cloned()
                    .unwrap_or_else(|| "var".to_string()),
            );
        }
        // Not a function-scope name, so it is object scope: a declared variable
        // or a property's backing variable. Falling through to `None` here made
        // every property argument look untyped, which silently disabled both the
        // numeric and the object upcast.
        let name = self.strings.get(index as usize)?;
        self.type_of(name).map(|t| t.0.clone())
    }

    /// Compile an expression, returning the value that holds its result.
    fn expr(&mut self, buf: &mut FnBuf, e: &Expr, line: u16) -> Result<Value, Error> {
        Ok(match e {
            Expr::Int(i) => Value::Int(*i),
            Expr::Float(f) => Value::Float(*f),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Str(s) => Value::String(self.intern(s)),
            Expr::None => {
                let n = self.none_var(buf);
                Value::Identifier(n)
            }
            Expr::Ident(n) => {
                let lower = n.to_ascii_lowercase();
                if let Some((_, i)) = buf
                    .params
                    .iter()
                    .chain(&buf.declared)
                    .find(|(p, _)| p == &lower)
                {
                    Value::Identifier(*i)
                } else if let Some(var) = self.auto_backing_var(n) {
                    // An `auto` property is not a name in the code: the
                    // property's storage is the `::Name_var` member of the
                    // object's variable list, and that is what instructions
                    // reference. Measured on DLC2ThirskDoorScript, where
                    // `Bujold.GetReference()` compiles to
                    // `callmethod GetReference ::Bujold_var ::temp2`.
                    Value::Identifier(self.intern(&var))
                } else {
                    Value::Identifier(self.intern(n))
                }
            }
            Expr::Binary { op, lhs, rhs } if op == "&&" || op == "||" => {
                self.logical(buf, op, lhs, rhs, line)?
            }
            Expr::Binary { op, lhs, rhs } => {
                let mut l = self.expr(buf, lhs, line)?;
                let mut r = self.expr(buf, rhs, line)?;
                // A comparison across two different script types casts the
                // right operand first, into its own temporary. Measured on
                // DLC2ThirskDoorScript, where `akActivator == Game.GetPlayer()`
                // compiles to `cast ::temp2 ::temp1` followed by
                // `cmp_eq ::temp3 akActivator ::temp2`.
                let is_cmp = matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=");
                // Arithmetic operands are converted the same way. `GameDaysPassed
                // .Value + 1` casts the int literal to Float before the `fadd`,
                // because the other operand is a Float — the conversion is not
                // special to comparisons. Restricted to numeric pairs: `+` on
                // strings is concatenation and casts nothing.
                let is_arith = matches!(op.as_str(), "+" | "-" | "*" | "/" | "%");
                let numeric_pair = {
                    let lt = self.value_type(buf, &l);
                    let rt = self.value_type(buf, &r);
                    let num =
                        |t: &str| t.eq_ignore_ascii_case("int") || t.eq_ignore_ascii_case("float");
                    num(&lt) && num(&rt)
                };
                if is_cmp || (is_arith && numeric_pair) {
                    let lt = self.value_type(buf, &l);
                    let rt = self.value_type(buf, &r);
                    // Compared case-insensitively: a local's declared type is
                    // interned as the source spelled it (`int`) while a literal
                    // yields the canonical `Int`, and an exact compare made
                    // `i < 3` cast an integer literal.
                    let same = lt.eq_ignore_ascii_case(&rt);
                    if !same && !lt.eq_ignore_ascii_case("var") && !rt.eq_ignore_ascii_case("var") {
                        // Which side is converted: **cast the right operand to
                        // the left's type by default**, and only flip when the
                        // left cannot accept the right but the right can accept
                        // the left. That is why `someInt < someFloat` casts the
                        // LEFT (int cannot absorb a float; float can absorb an
                        // int) while `someFloat == someInt` casts the right.
                        //
                        // Deciding this by "cast whichever is a temporary" was
                        // tried earlier and scored marginally worse, because the
                        // rule is about the types, not about which side happens
                        // to be a temp.
                        let cast_left = !self.widens_to(&lt, &rt) && self.widens_to(&rt, &lt);
                        let (want, src, to_left) = if cast_left {
                            (rt.clone(), l.clone(), true)
                        } else {
                            (lt.clone(), r.clone(), false)
                        };
                        let tmp = self.temp(buf, &want);
                        let mut ins = Instruction::new(opcode_of("cast"));
                        ins.args.push(Value::Identifier(tmp));
                        ins.args.push(src.clone());
                        buf.push(ins, line);
                        // The cast's operand is consumed by it and is dead
                        // afterwards. Not releasing it left the slot live for the
                        // rest of the function, so the next call wanting the same
                        // type took a fresh number: `Game.GetPlayer()` twice in
                        // `AltarOfAzuraScript.OnActivate` gives the game
                        // `::temp0` both times and gave us `::temp0` then
                        // `::temp4`.
                        self.release_temp(buf, &src);
                        // The cast's temporary goes back on the free list
                        // BEFORE the result is allocated, so the result reuses
                        // it. That ordering is what the compiler does: it
                        // creates the cast temp, marks it unused at once, marks
                        // both operands unused, and only then allocates the
                        // result — which therefore lands in the cast's slot.
                        self.release_temp(buf, &Value::Identifier(tmp));
                        if to_left {
                            l = Value::Identifier(tmp);
                        } else {
                            r = Value::Identifier(tmp);
                        }
                    }
                }
                // Both operands are dead once the instruction is formed, and
                // they are released before the result is allocated for the same
                // reason: the result must be free to take one of their slots.
                self.release_temp(buf, &l);
                self.release_temp(buf, &r);
                // A comparison yields Bool whatever its operands are; an
                // arithmetic instruction yields its operands' own type.
                let result_ty = match op.as_str() {
                    "+" | "-" | "*" | "/" | "%" => self.value_type(buf, &l),
                    _ => "Bool".to_string(),
                };
                let tmp = self.temp(buf, &result_ty);
                let code = match op.as_str() {
                    "+" => self.arith_op(buf, "+", &l),
                    "-" => self.arith_op(buf, "-", &l),
                    "*" => self.arith_op(buf, "*", &l),
                    "/" => self.arith_op(buf, "/", &l),
                    "%" => self.arith_op(buf, "%", &l),
                    "==" => opcode_of("cmp_eq"),
                    "!=" => opcode_of("cmp_eq"),
                    "<" => opcode_of("cmp_lt"),
                    "<=" => opcode_of("cmp_lte"),
                    ">" => opcode_of("cmp_gt"),
                    ">=" => opcode_of("cmp_gte"),
                    other => {
                        return Err(Error::Parse(format!(
                            "operator `{other}` is not compiled yet"
                        )))
                    }
                };
                let mut ins = Instruction::new(code);
                ins.args.push(Value::Identifier(tmp));
                ins.args.push(l.clone());
                ins.args.push(r.clone());
                buf.push(ins, line);
                // (operands were released above, before the result slot was
                // chosen — see the comment there.)
                if op == "!=" {
                    // `a != b` is `not (a == b)`, and the `not` writes **in
                    // place**: the comparison's result slot is released first so
                    // the negation takes the same one.
                    //   game  cmp_eq ::temp18 ... / not ::temp18 ::temp18
                    self.release_temp(buf, &Value::Identifier(tmp));
                    let tmp2 = self.temp(buf, "Bool");
                    let mut ins = Instruction::new(opcode_of("not"));
                    ins.args.push(Value::Identifier(tmp2));
                    ins.args.push(Value::Identifier(tmp));
                    buf.push(ins, line);
                    return Ok(Value::Identifier(tmp2));
                }
                Value::Identifier(tmp)
            }
            Expr::Unary { op, operand } => {
                let v = self.expr(buf, operand, line)?;
                let result_ty = match op.as_str() {
                    "!" => "Bool".to_string(),
                    _ => self.value_type(buf, &v),
                };
                // A unary operator writes **in place**: the operand is consumed,
                // so it goes back on the free list before the result is chosen
                // and the result takes its slot.
                //   `if (!IsInFaction(akFaction))`
                //   -> callmethod IsInFaction self ::temp18 akFaction
                //      not ::temp18 ::temp18
                // Allocating a fresh slot instead gave `not ::temp20 ::temp19`
                // and cost an extra Bool local in 2,175 functions.
                self.release_temp(buf, &v);
                let tmp = self.temp(buf, &result_ty);
                let code = match op.as_str() {
                    "!" => opcode_of("not"),
                    "-" => {
                        // Integer negation is `ineg`; float is `fneg`. The
                        // operand's own type decides, and a literal is int.
                        match &v {
                            Value::Float(_) => opcode_of("fneg"),
                            _ => opcode_of("ineg"),
                        }
                    }
                    other => {
                        return Err(Error::Parse(format!(
                            "unary operator `{other}` is not compiled yet"
                        )))
                    }
                };
                let mut ins = Instruction::new(code);
                ins.args.push(Value::Identifier(tmp));
                ins.args.push(v);
                buf.push(ins, line);
                Value::Identifier(tmp)
            }
            Expr::Cast(inner, ty) => {
                let v = self.expr(buf, inner, line)?;
                let tmp = self.temp(buf, &ty.0);
                let mut ins = Instruction::new(opcode_of("cast"));
                ins.args.push(Value::Identifier(tmp));
                ins.args.push(v);
                buf.push(ins, line);
                let _ = ty;
                Value::Identifier(tmp)
            }
            Expr::Call { callee, args } => {
                // Three shapes, and which one it is decides the opcode:
                //   `f(...)`            — callmethod on self
                //   `a.f(...)`          — callmethod on `a`
                //   `Class.f(...)`      — callstatic
                match callee.as_ref() {
                    Expr::Ident(name) => {
                        let fname = self.intern(name);
                        if let Some(target) = self.static_target(name) {
                            let tmp = self.temp(buf, "var");
                            let mut ins = Instruction::new(opcode_of("callstatic"));
                            ins.args.push(Value::Identifier(target));
                            ins.args.push(Value::Identifier(fname));
                            ins.args.push(Value::Identifier(tmp));
                            // Static calls pad their omitted trailing
                            // arguments exactly like method calls:
                            // `Debug.Trace("a")` emits
                            // `callstatic debug Trace ::NoneVar "a" 0`.
                            let own = self.own_class().unwrap_or_default();
                            let defs = crate::signatures::defaults(name);
                            // Arguments are evaluated RIGHT TO LEFT: the last
                            // argument's instructions come before the first's.
                            // Measured on `ArenaScript.ResolveWager`, where
                            // `AddItem(Gold, WagerAmount * 2)` emits the `imul`
                            // for argument 1 before the `cast` for argument 0.
                            // Positions in the vararg list stay in order; only
                            // the evaluation order is reversed.
                            let mut va: Vec<Option<Value>> = vec![None; args.len()];
                            for (i, a) in args.iter().enumerate().rev() {
                                let v = self.expr(buf, a, line)?;
                                let v = match crate::signatures::arg_type(&own, name, i) {
                                    Some(w) => self.coerce_arg(buf, v, w, line),
                                    None => v,
                                };
                                va[i] = Some(v);
                            }
                            let va: Vec<Value> = va.into_iter().map(|v| v.unwrap()).collect();
                            ins.varargs = self.pad_defaults(va, defs, name);
                            buf.push(ins, line);
                            return Ok(Value::Identifier(tmp));
                        }
                        let self_i = self.intern("self");
                        let ty = if let Some(c) = self.own_class() {
                            self.static_result_type(&c, name)
                        } else {
                            self.call_result_type(name)
                        };
                        let ty = if ty == "var" {
                            self.call_result_type(name)
                        } else {
                            ty
                        };
                        let tmp = self.temp(buf, ty);
                        let mut ins = Instruction::new(opcode_of("callmethod"));
                        ins.args.push(Value::Identifier(fname));
                        ins.args.push(Value::Identifier(self_i));
                        ins.args.push(Value::Identifier(tmp));
                        // A bare `f(...)` is a call on `self`, and its arguments
                        // are converted exactly like a member call's. This path
                        // had padding but no coercion, so `SetForcedLandingMarker(None)`
                        // passed `::NoneVar` where the game emits
                        // `cast ::temp7 None` first — the difference in ~2,290
                        // functions, because `None` is how Papyrus spells an
                        // unset object.
                        let own = self.own_class().unwrap_or_default();
                        // NOTE: a bare `f(...)` evaluates its arguments LEFT to
                        // RIGHT, unlike a member call. Reversing this scores
                        // 43,603 against 43,605, so the two paths genuinely
                        // differ.
                        let mut dealt = Vec::with_capacity(args.len());
                        for (i, a) in args.iter().enumerate() {
                            let v = self.expr(buf, a, line)?;
                            let v = match crate::signatures::arg_type(&own, name, i) {
                                Some(w) => self.coerce_arg(buf, v, w, line),
                                None => v,
                            };
                            dealt.push(v);
                        }
                        let defs = crate::signatures::defaults(name);
                        ins.varargs = self.pad_defaults(dealt, defs, name);
                        buf.push(ins, line);
                        Value::Identifier(tmp)
                    }
                    Expr::Member(obj, method) => {
                        // `Game.GetPlayer()` parses as a member access, but the
                        // engine's own classes are not objects: the CK emits
                        // `callstatic game GetPlayer ::temp1`, with the class
                        // lowercase and the function second. Emitting
                        // `callmethod` here instead is accepted by the loader
                        // and then fails at run time, so the check has to come
                        // before the receiver is compiled.
                        // `parent.Foo(...)` is `callparent`, a distinct opcode
                        // with **no receiver operand** — the receiver is
                        // implicit, so the argument list is `method dest args…`.
                        // Emitting `callmethod Foo parent …` looks up a variable
                        // named `parent` and fails at run time.
                        //
                        // Only 66 sites in the whole corpus against 115,365
                        // `callmethod`, so no census ever ranked it; it is
                        // invisible rather than rare.
                        if let Expr::Ident(n) = obj.as_ref() {
                            if n.eq_ignore_ascii_case("parent") {
                                let m = self.intern(method);
                                let ty = self.call_result_type(method);
                                let tmp = self.temp(buf, ty);
                                let mut ins = Instruction::new(opcode_of("callparent"));
                                ins.args.push(Value::Identifier(m));
                                ins.args.push(Value::Identifier(tmp));
                                let mut slots: Vec<Option<Value>> = vec![None; args.len()];
                                for (i, a) in args.iter().enumerate().rev() {
                                    slots[i] = Some(self.expr(buf, a, line)?);
                                }
                                let va: Vec<Value> =
                                    slots.into_iter().map(|v| v.unwrap()).collect();
                                // Defaults come from the PARENT class, not the
                                // unqualified name. `parent.Cleanup()` takes no
                                // arguments, but `cleanup` is also declared with a
                                // `bool` elsewhere in the corpus, so padding from
                                // the unqualified table appended a phantom `false`
                                // the game does not have. Walk the `extends` chain
                                // from the script's parent and pad only from a
                                // declaration that actually reaches the call.
                                let mut here = self.own_class();
                                let mut defs = None;
                                for _ in 0..12 {
                                    let Some(c) = here else { break };
                                    if let Some(d) = crate::signatures::method_defaults(&c, method)
                                    {
                                        defs = Some(d);
                                        break;
                                    }
                                    here = crate::signatures::parent(&c).map(str::to_string);
                                }
                                ins.varargs = self.pad_defaults(va, defs, method);
                                let released: Vec<Value> = ins.varargs.clone();
                                buf.push(ins, line);
                                for a in &released {
                                    self.release_temp(buf, a);
                                }
                                return Ok(Value::Identifier(tmp));
                            }
                        }
                        if let Expr::Ident(class) = obj.as_ref() {
                            if let Some(target) = self.static_target(class) {
                                let m = self.intern(method);
                                // The result's type decides the temporary, and
                                // that decides whether a later comparison needs
                                // a `cast`. Measured, not guessed.
                                let ty = self.static_result_type(class, method);
                                let tmp = self.temp(buf, ty);
                                let mut ins = Instruction::new(opcode_of("callstatic"));
                                ins.args.push(Value::Identifier(target));
                                ins.args.push(Value::Identifier(m));
                                ins.args.push(Value::Identifier(tmp));
                                let defs = crate::signatures::defaults(method);
                                let cls = class.to_ascii_lowercase();
                                let sig = crate::signatures::method_params(&cls, method);
                                let mut va = Vec::with_capacity(args.len());
                                for (i, a) in args.iter().enumerate() {
                                    let v = self.expr(buf, a, line)?;
                                    let v = match sig.and_then(|s| s.get(i)) {
                                        Some(w) => self.coerce_arg(buf, v, w, line),
                                        None => v,
                                    };
                                    va.push(v);
                                }
                                ins.varargs = self.pad_defaults(va, defs, method);
                                buf.push(ins, line);
                                return Ok(Value::Identifier(tmp));
                            }
                        }
                        // The receiver's declared type tells us which class the
                        // method belongs to, so the corpus table can supply the
                        // result type. Without it the temporary is `var`, and
                        // the corpus shows `var` for only a handful of the
                        // 4,000+ call results it records.
                        let recv_class = match obj.as_ref() {
                            Expr::Ident(n) => self.type_of(n).map(|t| t.0.clone()),
                            Expr::Member(..) => None,
                            _ => None,
                        };
                        // NOTE: there is no special "materialise the receiver"
                        // step. `Self.Activate(Self, true)` compiles to
                        //   cast ::temp2 self
                        //   callmethod Activate self ::temp3 ::temp2 Bool(true)
                        // so `self` IS cast — but as ARGUMENT 0 for `Activate`'s
                        // `ObjectReference` parameter, not because it is the
                        // receiver. An earlier version cast it before every
                        // `Self.<method>()` call, which put a `cast` on calls the
                        // CK leaves alone (`Self.MoveTo(...)` has none).
                        let recv = self.expr(buf, obj, line)?;
                        let m = self.intern(method);
                        let ty = match &recv_class {
                            Some(c) => {
                                // Look the result type up on the RECEIVER's
                                // class first: `AtronachFrostFXS.Play(...)` has a
                                // receiver of type EffectShader, so the answer is
                                // `effectshader.play`, which is None. Consulting
                                // only the script's own class missed it, fell
                                // back to `var`, and allocated a temporary where
                                // the game writes ::NoneVar.
                                let t = self.static_result_type(c, method);
                                if t == "var" {
                                    crate::signatures::return_type(c, method)
                                        .unwrap_or_else(|| self.call_result_type(method))
                                } else {
                                    t
                                }
                            }
                            None => self.call_result_type(method),
                        };
                        let tmp = self.temp(buf, ty);
                        let mut ins = Instruction::new(opcode_of("callmethod"));
                        ins.args.push(Value::Identifier(m));
                        ins.args.push(recv.clone());
                        ins.args.push(Value::Identifier(tmp));
                        // Arguments are converted type-directed: an int in a
                        // float slot gets a `cast`, everything else is left
                        // alone. Measured with the real compiler, not guessed.
                        // Resolve through the **class chain**, not just the
                        // receiver's own class: `AddItem` is declared on
                        // `ObjectReference` and reached from an `Actor`, and
                        // `additem` is absent from the unqualified table because
                        // `FormArray.AddItem(Form)` and
                        // `ObjectReference.AddItem(Form, int, bool)` disagree.
                        // Without the walk the call got neither its argument
                        // cast nor its trailing `false`.
                        let class = recv_class.as_deref().unwrap_or("");
                        // The per-position table covers names whose whole
                        // signature was dropped as ambiguous, so only the
                        // defaults need looking up here.
                        let defs =
                            crate::signatures::method_signature(class, method).map(|(_, d)| d);
                        // Arguments are evaluated RIGHT TO LEFT: the last
                        // argument's instructions are emitted before the first's.
                        // Measured on `ArenaScript.ResolveWager`, where
                        // `AddItem(Gold, WagerAmount * 2)` emits the `imul` for
                        // argument 1 before the `cast` for argument 0, then
                        // passes them in the correct positions. Only evaluation
                        // order is reversed; the vararg list keeps source order.
                        let mut slots: Vec<Option<Value>> = vec![None; args.len()];
                        for (i, a) in args.iter().enumerate().rev() {
                            let v = self.expr(buf, a, line)?;
                            let v = match crate::signatures::arg_type(class, method, i) {
                                Some(want) => self.coerce_arg(buf, v, want, line),
                                None => v,
                            };
                            slots[i] = Some(v);
                        }
                        let va: Vec<Value> = slots.into_iter().map(|v| v.unwrap()).collect();
                        // Omitted trailing arguments are written out, not
                        // dropped: the .pex must carry the count the game has.
                        ins.varargs = self.pad_defaults(va, defs, method);
                        let released: Vec<Value> = ins.varargs.clone();
                        buf.push(ins, line);
                        // A temporary consumed as an argument or receiver is
                        // released once the call is emitted. The CK frees the
                        // receiver **before** the arguments, which is what makes
                        // a loop of `A.GetReference().MoveTo(B.GetReference())`
                        // alternate ::temp0 ::temp1 from one statement to the
                        // next instead of counting upward forever.
                        self.release_temp(buf, &recv);
                        for a in &released {
                            self.release_temp(buf, a);
                        }
                        Value::Identifier(tmp)
                    }
                    other => {
                        return Err(Error::Parse(format!(
                            "cannot call {other:?}: only a name or a member is callable"
                        )))
                    }
                }
            }
            Expr::Member(obj, name) => {
                // A bare member read is a property get.
                //
                // The operand order is **name, object, destination**, not
                // destination-first. Measured over all 6,630 `propget`
                // instructions in the shipped corpus: slot 2 is a temporary
                // 6,630/6,630 times (it is the destination), slot 1 is a
                // temporary only 1,160 times (it is the object, usually a
                // property's backing variable), and slot 0 is never a temporary
                // (it is the property name). `examples/propargs.rs` counts it.
                //
                // An earlier `proptags` probe read slot 2 and found an
                // `Identifier` 9,982 times, which looked like confirmation but
                // was not: a `::tempN` is an identifier too, so that count was
                // consistent with both orders.
                let o = self.expr(buf, obj, line)?;
                let p = self.intern(name);
                // The property may belong to ANOTHER class: `aScript.CurrFight`
                // reads a property declared on ArenaScript. The receiver's own
                // declared type says which class to ask, and the answer is what
                // gives the read a type at all — an untyped value cannot be
                // converted into the slot it is written to, so the cast the game
                // emits would be missing.
                let pty = match obj.as_ref() {
                    Expr::Ident(n) => {
                        // The receiver may be a LOCAL (`ArenaScript aScript`,
                        // declared inside the function) as well as a script-level
                        // variable. Asking only the script-level table left
                        // `aScript.NextWagerFight` untyped, so the value it
                        // produced could not be converted into the `quest` slot it
                        // was assigned to and the cast the game emits was missing.
                        let cls = self.receiver_class(buf, n).unwrap_or_default();
                        crate::signatures::property_type(&cls, name)
                            .unwrap_or_else(|| self.property_type(name))
                    }
                    _ => self.property_type(name),
                };
                let tmp = self.temp(buf, pty);
                let mut ins = Instruction::new(opcode_of("propget"));
                ins.args.push(Value::Identifier(p));
                ins.args.push(o);
                ins.args.push(Value::Identifier(tmp));
                buf.push(ins, line);
                // NOTE: the receiver temp is deliberately NOT released here.
                // Releasing it scores 42,339 against 42,341, so either the slot
                // is still live for the caller or the release belongs somewhere
                // else in the property path. Recorded rather than kept.
                Value::Identifier(tmp)
            }
            Expr::Index(arr, idx) => {
                let a = self.expr(buf, arr, line)?;
                let i = self.expr(buf, idx, line)?;
                let tmp = self.temp(buf, "var");
                let mut ins = Instruction::new(opcode_of("array_getelement"));
                ins.args.push(Value::Identifier(tmp));
                ins.args.push(a);
                ins.args.push(i);
                buf.push(ins, line);
                // NOTE: array operands are not released here either — same
                // measurement, same result.
                Value::Identifier(tmp)
            }
        })
    }

    /// The class a `Class.Function(...)` call is static on, if it is one.
    ///
    /// A word that names a property of a known script type, or a capitalised
    /// name that is neither a local nor a parameter, is a class name. This is a
    /// heuristic because a Papyrus compile unit is one file: the parent's
    /// members are not visible here. It is honest about being one, and a
    /// mis-resolution produces a `callstatic` on a name the game then rejects
    /// rather than a silently wrong value.
    fn static_target(&mut self, name: &str) -> Option<StrIndex> {
        let lower = name.to_ascii_lowercase();
        // The engine's own classes are spelled lowercase in a `.pex` whatever
        // the source says: `Game.GetPlayer()` compiles to `callstatic` on
        // `game`, not `Game`. Measured on DLC2ThirskDoorScript.
        // `self` is NOT a static class: `Self.Activate(...)` is a `callmethod`
        // on the receiver. Only the engine's own classes are static.
        if matches!(lower.as_str(), "game" | "debug" | "utility" | "math") {
            return Some(self.intern(&lower));
        }
        // A property whose type is a script class is **not** a static
        // receiver: `Scene Property ExileScene auto` makes `ExileScene` an
        // *instance*, and `ExileScene.IsPlaying()` is a `callmethod` on it.
        // Treating it as a class emitted `callstatic`, which the loader accepts
        // and the VM then runs against the wrong receiver. Measured on
        // DLC2ThirskDoorScript, where the game emits
        // `callmethod IsPlaying ::ExileScene_var ::temp5`.
        None
    }
}

/// An opcode by mnemonic.
fn opcode_of(name: &str) -> u8 {
    for (i, op) in opcodes::SKYRIM.iter().enumerate() {
        if op.name == name {
            return i as u8;
        }
    }
    // Every name passed here is a constant in this file, so a miss is a bug in
    // this crate rather than in the input.
    debug_assert!(false, "no opcode named {name}");
    0
}

/// The functions a script declares, for a caller that wants to know what a
/// compile produced without walking the states.
pub fn declared_functions(s: &Script) -> Vec<&FunctionDecl> {
    let mut v = Vec::new();
    for d in &s.decls {
        if let DeclKind::Function(f) = &d.kind {
            v.push(f);
        }
    }
    v
}

/// The properties a script declares.
pub fn declared_properties(s: &Script) -> Vec<&Decl> {
    s.decls
        .iter()
        .filter(|d| matches!(d.kind, DeclKind::Property(_)))
        .collect()
}

fn canonical_event_name(name: &str) -> &str {
    if name.eq_ignore_ascii_case("oninit") {
        "OnInit"
    } else if name.eq_ignore_ascii_case("onbeginstate") {
        "onBeginState"
    } else if name.eq_ignore_ascii_case("onendstate") {
        "onEndState"
    } else {
        name
    }
}

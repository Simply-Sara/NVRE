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

/// Whether `PAPYRUS_TRACE` is set, read once.
///
/// Allocation and release happen for every temporary in every function, so the
/// variable is resolved through a `OnceLock` rather than a syscall per call.
fn tracing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PAPYRUS_TRACE").is_some())
}

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
    /// Consulted by [`Compiler::type_of`] after the function and script scopes,
    /// so a name the caller knows about — an inherited member, or anything from
    /// another script in the same mod — resolves to a type instead of `None`.
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

    /// The declared type of a name, or `None`.
    ///
    /// Lookup order, and the order matters:
    ///
    /// 1. **This function's** scope — parameters and locals, which shadow
    ///    everything else.
    /// 2. **This script's** declarations, which are in `self.types`.
    /// 3. **The caller-supplied symbol table** — the script's own variables and
    ///    properties as the *caller* declared them.
    ///
    /// Step 3 is what an editor needs and what compiling a single file cannot
    /// supply. Compiling one `.psc` in isolation, a name inherited from a parent
    /// class is invisible, so it types as nothing, no conversion fires at the
    /// slot it is written to, and the arithmetic opcode is chosen from `var`.
    /// Compiling a mod as one unit fills the table from every script in the mod.
    ///
    /// Measured before this existed: with `Symbols` never read, the emitter's
    /// answer for an inherited name was always `None`, and `Symbols` carried
    /// `#[allow(dead_code)]` as a marker of that.
    fn type_of(&self, name: &str) -> Option<&TypeName> {
        let lower = name.to_ascii_lowercase();
        // A property's **backing member** carries the property's own type.
        // `::mySelf_var` IS the storage the declaration made, so it is a
        // `miscobject` and nothing else is. Reading it as `var` means no
        // conversion fires at a slot it fills: `BYOHBuildingObjectExteriorScript
        // .OnContainerChanged` passes `myself` — `MiscObject Property mySelf
        // Auto`, inherited from `BYOHBuildingObjectScript` — where the parameter
        // wants a `Form`, and the game writes `cast ::temp5 ::mySelf_var` where
        // this wrote the variable bare, one operand short.
        //
        // The search covers both lists because which one a name lands in depends
        // on where it was declared: an ancestor's members arrive through
        // `ClassTable` as `variables`, a script's own declarations as
        // `properties` or `types`. The type is the same either way.
        let bare = lower
            .strip_prefix("::")
            .and_then(|n| n.strip_suffix("_var"))
            .unwrap_or(&lower);
        let mut keys: Vec<&str> = vec![&lower];
        if bare != lower {
            keys.push(bare);
        }
        for key in keys {
            if let Some((_, t)) = self.types.iter().find(|(n, _)| n == key) {
                return Some(t);
            }
            if let Some((_, t)) = self.symbols.variables.iter().find(|(n, _)| n == key) {
                return Some(t);
            }
            if let Some((_, t)) = self.symbols.properties.iter().find(|(n, _)| n == key) {
                return Some(t);
            }
        }
        None
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
        // A parameter costs the temporary counter **nothing**, read or unread.
        //
        // An earlier note here claimed an unread parameter consumes two numbers,
        // from five hand-built probes that seemed to show
        // `Function One(int n)` starting at `::temp2`. `examples/parambase`
        // measures the relation over the 12,432 shipped functions that have both
        // parameters and temporaries, and the base is `0` in the overwhelming
        // majority for EVERY (params, unread) combination — including 1,047
        // one-parameter functions (638 at base 0) and 8 six-parameter functions
        // with five unread (all 8 at base 0). The spread around it is the
        // script-wide counter, not a parameter effect.
        //
        // So the probes were reading a neighbour's drift: the counter is
        // per-script and monotonic, so a hand-built file with one function cannot
        // show the rule at all. Do not reintroduce an offset here without
        // re-running that probe.
        for p in &f.params {
            let pn = self.intern(&p.name);
            let pt = self.intern(&p.type_name.0);
            buf.types.push((pn, pt));
            buf.params.push((p.name.to_ascii_lowercase(), pn));
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
        if tracing() {
            let names: Vec<String> = buf
                .free_temps
                .iter()
                .map(|(i, _)| self.strings.get(*i as usize).cloned().unwrap_or_default())
                .collect();
            eprintln!("  alloc {ty:<16} free={names:?}");
        }
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
            if tracing() {
                eprintln!(
                    "  free  {}",
                    self.strings.get(*i as usize).cloned().unwrap_or_default()
                );
            }
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
                    // The slot the value is written into is the PROPERTY's
                    // declared type, not the value's own. Measured on
                    // `(GetOwningQuest() as BYOHRelationshipAdoptionScript)
                    // .OrderToConfirm = 2`, where OrderToConfirm is a Float and
                    // the literal is an Int:
                    //   cast ::temp4 2
                    //   assign ::temp3 ::temp4
                    //   propset OrderToConfirm ::temp2 ::temp3
                    // Two instructions: the conversion, then the copy into the
                    // value slot. Coercing to the value's own type emitted a
                    // bare `assign ::temp1 2` and no cast at all.
                    let pty = self
                        .receiver_class_of(buf, obj)
                        .and_then(|cls| {
                            crate::signatures::property_type(&cls, name).map(str::to_string)
                        })
                        .or_else(|| Some(self.property_type(name)));
                    // The value slot is allocated BEFORE the conversion, so the
                    // cast's own temporary takes the HIGHER number. Measured on
                    // `CWScript.OnInit`, `debugOn = 0` where `debugOn` is a
                    // Float property and the literal an Int:
                    //
                    //   game:  cast ::temp1 0                 <- cast allocated second
                    //          assign ::temp0 ::temp1
                    //          propset value ::debugOn_var ::temp0
                    //   ours:  cast ::temp0 0                 <- cast allocated first
                    //          assign ::temp1 ::temp0
                    //          propset value ::debugOn_var ::temp1
                    //
                    // Same three instructions, both slots exchanged — and because
                    // the counter is script-wide, every later function in a
                    // 400-function script inherited the swap.
                    let pre_slot = pty.as_ref().map(|t| self.temp(buf, t));
                    let raw = self.expr(buf, value, *line)?;
                    let raw = match &pty {
                        Some(t) => self.coerce_arg(buf, raw, t, *line),
                        None => raw,
                    };
                    // The VALUE is materialised into a temporary, always: slot 2
                    // of all 3,352 shipped `propset` instructions is a `::tempN`
                    // and never a literal. `kmyQuest.SomeProp = true` compiles to
                    //   assign ::temp17 true
                    //   propset SomeProp kmyQuest ::temp17
                    // and the CK reuses that one temp for consecutive propsets,
                    // so releasing it after the instruction is what makes the
                    // next one take the same slot.
                    let ty = match &pty {
                        Some(t) => t.clone(),
                        None => self.value_type(buf, &raw),
                    };
                    // The cast's own destination, when the value needed
                    // converting: it is dead once the copy into the value slot is
                    // emitted, and freeing it there is what makes the NEXT
                    // property write reuse it.
                    let mut cast_slot = None;
                    let v = match pre_slot {
                        Some(slot) => {
                            if let Value::Identifier(r) = &raw {
                                if r != &slot
                                    && self
                                        .strings
                                        .get(*r as usize)
                                        .map(|n| n.starts_with("::temp"))
                                        .unwrap_or(false)
                                {
                                    cast_slot = Some(*r);
                                }
                            }
                            let mut ins = Instruction::new(opcode_of("assign"));
                            ins.args.push(Value::Identifier(slot));
                            ins.args.push(raw);
                            buf.push(ins, *line);
                            Value::Identifier(slot)
                        }
                        None => self.materialise_copy(buf, raw, &ty, *line),
                    };
                    let o = self.expr(buf, obj, *line)?;
                    let mut ins = Instruction::new(opcode_of("propset"));
                    ins.args.push(Value::Identifier(p));
                    ins.args.push(o);
                    ins.args.push(v.clone());
                    buf.push(ins, *line);
                    // The materialised value is dead once the property is
                    // written, and the CK reuses that one slot for consecutive
                    // propsets (`assign ::temp17 true` … `assign ::temp17 true`).
                    //
                    // The CAST's slot is released FIRST, and both go back before
                    // the next statement allocates. Measured on `CWScript.OnInit`,
                    // where `baseSalary = 100` (a Float property, an Int literal)
                    // is followed two statements later by another Float cast:
                    //
                    //   cast ::temp1 100 / assign ::temp0 ::temp1 / propset …
                    //   … cast ::temp1 100          <- the CAST's slot, reused
                    //
                    // The free list is FIFO per type, so freeing the cast before
                    // the value slot is what hands `::temp1` back out. Not
                    // releasing the cast slot at all gave `cast ::temp0 100`, and
                    // every later Float temporary in a 400-function script moved.
                    if let Some(c) = cast_slot {
                        self.release_temp(buf, &Value::Identifier(c));
                    }
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
                // `a.b += v` is not a plain read-modify-write of a name: the
                // property is read with `propget`, the sum stored in a
                // temporary, and the result written back with `propset`.
                // `lvalue` answers a member target with the property NAME, which
                // is right for `propset`'s first operand and useless as an
                // arithmetic operand — taking it emitted
                // `fadd ::temp0 Value 1 ; assign Value ::temp0`, an assignment to
                // a name the VM has no slot for.
                //
                // Measured on `AbTGTrapsightScript.OnEffectStart`, whose whole
                // body is `pTSight.Value += 1`:
                //
                //   0 propget value ::pTSight_var ::temp1   the read
                //   1 cast    ::temp2 1                     the int literal, widened
                //   2 fadd    ::temp2 ::temp1 ::temp2       the sum reuses the cast's slot
                //   3 assign  ::temp0 ::temp2               into the propset value slot
                //   4 propset value ::pTSight_var ::temp0
                //
                // `::temp0` is the LOWEST number in the function, so the value
                // slot is allocated before the expression is evaluated — the same
                // "materialise the value first" rule the plain `a.b = v` path
                // already follows.
                if let Expr::Member(obj, name) = target {
                    let p = self.intern(name);
                    let pty = self
                        .receiver_class_of(buf, obj)
                        .and_then(|cls| {
                            crate::signatures::property_type(&cls, name).map(str::to_string)
                        })
                        .or_else(|| Some(self.property_type(name)));
                    let ty = pty.unwrap_or_else(|| "var".to_string());
                    // The receiver is evaluated FIRST, then the value slot, then
                    // the read's destination. Measured on `TIF__0004E4C8
                    // .Fragment_0`, `(GetOwningQuest() as DA09Script)
                    // .GemSceneStage += 1`, whose locals run
                    // `::temp1:quest  ::temp2:da09script  ::temp3:Int
                    //  ::temp4:Int` — receiver, cast, value slot, propget
                    // destination. Allocating the value slot before the receiver
                    // put it in `::temp1` and displaced the other three.
                    let o = self.expr(buf, obj, *line)?;
                    let v_slot = self.temp(buf, &ty);
                    let cur = self.temp(buf, &ty);
                    let mut ins = Instruction::new(opcode_of("propget"));
                    ins.args.push(Value::Identifier(p));
                    ins.args.push(o.clone());
                    ins.args.push(Value::Identifier(cur));
                    buf.push(ins, *line);
                    let rhs = self.expr(buf, value, *line)?;
                    let rhs = self.coerce_arg(buf, rhs, &ty, *line);
                    // The cast's slot goes back on the free list before the sum
                    // is allocated, so the sum lands in it — the same ordering
                    // the comparison path needs.
                    // Both operands are released before the sum's destination is
                    // chosen, so the sum lands in one of their slots — the same
                    // release-before-allocate ordering the comparison path needs.
                    // `GemSceneStage += 1` writes `iadd ::temp4 ::temp4 1`: the
                    // read's own slot, rewritten in place. Allocating after only
                    // the cast was released gave `iadd ::temp5 ::temp4 1`.
                    self.release_temp(buf, &rhs);
                    self.release_temp(buf, &Value::Identifier(cur));
                    let dest = self.temp(buf, &ty);
                    let arith = self.arith_op(buf, op, &Value::Identifier(cur));
                    let mut ins = Instruction::new(arith);
                    ins.args.push(Value::Identifier(dest));
                    ins.args.push(Value::Identifier(cur));
                    ins.args.push(rhs);
                    buf.push(ins, *line);
                    let mut ins = Instruction::new(opcode_of("assign"));
                    ins.args.push(Value::Identifier(v_slot));
                    ins.args.push(Value::Identifier(dest));
                    buf.push(ins, *line);
                    let mut ins = Instruction::new(opcode_of("propset"));
                    ins.args.push(Value::Identifier(p));
                    ins.args.push(o);
                    ins.args.push(Value::Identifier(v_slot));
                    buf.push(ins, *line);
                    self.release_temp(buf, &Value::Identifier(dest));
                    self.release_temp(buf, &Value::Identifier(v_slot));
                    return Ok(());
                }
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
            // **except the last branch when it is an `else`** — an `else` has no
            // condition of its own, so falling into it is the only way it runs
            // and the fall-through already lands past the chain.
            //
            // That one rule replaces three narrower ones that had been fitted to
            // individual witnesses, and it explains every one of them:
            //
            // - `ActivateAgainAfterDelay.OnActivate` — `if … else … endif` whose
            //   else ends with `gotoState("pulledPosition")`: no jump, because
            //   the final branch is an `else`.
            // - `ArenaCombatQuest.ShouldEnd` — `if/else`, both `return`: the
            //   then-branch keeps its `jmp`, the else has none. Same rule; the
            //   earlier reading ("the last branch drops it when it ends in
            //   `return`") mistook the cause for the effect.
            // - `Actor.MakePlayerFriend` — inner and outer `if`s that both have a
            //   comment-only `else`: two jumps in the whole body, both then
            //   branches.
            // - A single-branch `if c endif` keeps its jump even when empty
            //   (`jmpf c 2 ; jmp 1`), and `if c return endif` keeps it too
            //   (`CWSoldierDeathObserverActorScript.OnDeath`) — neither is an
            //   `else`, so nothing is elided.
            // - `AchievementsScript.IncHardWorker` — `if/elseif/elseif` ending in
            //   a nested chain: the last branch DOES emit its jump, and the whole
            //   `operand value on jmp`/`jmpf` bucket (757 functions) was this
            //   one omission. A chain whose last branch is an `elseif` is not an
            //   `else`.
            //
            // Measured both ways over 54,620 functions: skipping the last branch
            // unconditionally scores 2,106 behavioural differences, eliding
            // nothing scores 2,579, and this rule scores 1,566.
            let is_last = mine_branch + 1 == branches.len();
            let is_else = b.cond.is_none();
            // An `else` whose body is empty, or which ends in `return`, needs no
            // jump either — but both fall out of the same test, because an empty
            // body emits nothing to jump over and the CK never emits a jump whose
            // target is the next instruction.
            let skip = is_last && is_else;
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

    /// Like [`materialise`], but copies even when the value is already a
    /// temporary.
    ///
    /// A `propset` value goes through **two** steps when it needs converting:
    /// the conversion produces a temporary and the value slot is a second one.
    /// See the `cast ::temp4 2` / `assign ::temp3 ::temp4` measurement at the
    /// `propset` site.
    fn materialise_copy(&mut self, buf: &mut FnBuf, v: Value, ty: &str, line: u16) -> Value {
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
            Expr::Member(_, name) => Some(self.property_type(name)),
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
                // `auto_backing_var` covers an INHERITED `auto` property as
                // well as a declared one: the storage is the object's, so
                // `IsTransitionFight = x` in a subclass writes
                // `::IsTransitionFight_var`.
                if let Some(backing) = self.auto_backing_var(n) {
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
        if let Some(b) = self.script.decls.iter().find_map(|d| match &d.kind {
            DeclKind::Property(p) if p.is_auto && p.name.eq_ignore_ascii_case(name) => {
                Some(format!("::{}_var", p.name))
            }
            _ => None,
        }) {
            return Some(b);
        }
        // An INHERITED `auto` property is not in this script's declaration list,
        // but it is still backed by `::Name_var` — the storage belongs to the
        // object, not to the file. `IsTransitionFight = isTransition` in
        // `ArenaTransitionCombatQuest`, whose `IsTransitionFight` comes from
        // `ArenaCombatQuest`, compiled to `assign IsTransitionFight …` where the
        // game writes `assign ::IsTransitionFight_var …`.
        //
        // A script VARIABLE of the same name shadows the property, so it is
        // checked first.
        let n = name.to_ascii_lowercase();
        let is_var = self.script.decls.iter().any(|d| match &d.kind {
            DeclKind::Variable(v) => v.name.eq_ignore_ascii_case(name),
            _ => false,
        });
        if is_var {
            return None;
        }
        let mut here = self.own_class();
        for _ in 0..12 {
            let Some(c) = here else { break };
            if let Some(spelling) = crate::signatures::auto_property_name(&c, &n) {
                // Use the spelling the class declares, so `::Name_var` matches
                // the member the object actually has: the slot is
                // `::IsTransitionFight_var`, not the lowercased source word.
                return Some(format!("::{spelling}_var"));
            }
            here = crate::signatures::parent(&c).map(str::to_string);
        }
        None
    }

    /// The type this script `extends`, which is the class a bare `f()` call
    /// implicitly dispatches through.
    fn own_class(&self) -> Option<String> {
        // The source's own `extends` first; then whatever the caller supplied,
        // which is how an editor compiling a script whose parent is declared
        // elsewhere still gets the chain.
        self.script
            .parent
            .clone()
            .or_else(|| Some(self.symbols.parent.clone()).filter(|p| !p.is_empty()))
    }

    /// The declared type of a property, for its `propget` temporary.
    /// A property's declared type as a slot type.
    ///
    /// The eight words this returns are the ones the corpus spells in a locals
    /// table: the four primitives capitalised, and the class name **as declared**.
    /// An earlier version mapped every class to `var`, which is the same
    /// "typed slot answering var" defect that cost conversions at call arguments
    /// and array reads: `kmyquest.CWs.WhiterunLocation` is a
    /// `Location Property WhiterunLocation` on `CWScript`, the game types the
    /// `propget` destination `::temp17:location`, and a `var` slot could not be
    /// compared against `cityVar` (a `location` parameter) without an extra cast
    /// — and it took a different temporary, displacing the rest of a 900-line
    /// quest fragment.
    ///
    /// A class name is normalised to lowercase, which is how the corpus spells a
    /// class in a slot; primitives stay capitalised. That split is measured, not
    /// cosmetic — see `var_type`.
    fn property_type(&self, name: &str) -> String {
        match self.type_of(name) {
            Some(t) => match t.base().as_str() {
                "int" => "Int".to_string(),
                "float" => "Float".to_string(),
                "bool" => "Bool".to_string(),
                "string" => "String".to_string(),
                "none" | "var" => "var".to_string(),
                other => other.to_ascii_lowercase(),
            },
            None => "var".to_string(),
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
        ins.args.push(b.clone());
        buf.push(ins, line);
        // The right operand is consumed by that cast and is dead. Leaving it
        // live kept its slot occupied for the rest of the function:
        // `AltarOfMolagBalScript.OnAnimationEvent` has `cmp_eq ::temp3 …` /
        // `cast ::temp2 ::temp3`, and the game reuses ::temp3 as the very next
        // call's destination while we allocated ::temp4.
        self.release_temp(buf, &b);

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
    /// A declared default as the value the instruction carries.
    ///
    /// `bool abMovement = true` becomes `Value::Bool(true)`; a `str` default is
    /// interned. `None` for a kind this does not know, so the caller can stop
    /// rather than emit a wrong literal.
    fn default_value(&mut self, kind: &str, text: &str) -> Option<Value> {
        Some(match kind {
            "bool" => Value::Bool(text == "true"),
            "none" => Value::None,
            "int" => Value::Int(text.parse::<i32>().ok()?),
            "float" => Value::Float(text.parse::<f32>().ok()?),
            "str" => Value::String(self.intern(text)),
            _ => return None,
        })
    }

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
            let Some(v) = self.default_value(kind, text) else {
                break;
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
        // An unknown source type is not evidence of a conversion — see
        // `known_type`. The fallback `value_type` gives for an undeclared name
        // is `Int`/`Float`, and reading that as knowledge invents casts the CK
        // does not emit.
        let Some(have) = self.known_type(buf, &value) else {
            return value;
        };
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

    /// Where each argument goes, when the source named some of them.
    ///
    /// `Game.DisablePlayerControls(abMovement = false, abMenu = false,
    /// abActivate = false, abFighting = true)` writes **all nine** parameters of
    /// the declaration: the four named values at their declared slots and the
    /// other five at their defaults. Passing the four positionally puts `false`
    /// in `abMovement` and `false` in `abFighting`, so the call runs with the
    /// wrong booleans — a behaviour bug, not a numbering one.
    ///
    /// Returns `None` when no argument is named (the common case, and the one
    /// that must not pay for a table lookup), or when the callee's parameter
    /// names are unknown — in which case the positional reading is the only one
    /// available and is left alone rather than guessed at.
    ///
    /// An unnamed argument fills the lowest slot not already taken by a name, in
    /// source order, which is what the language specifies and what the corpus
    /// shows: `ShowGiftMenu(abGivingGift = true, akSpeaker = None, …)` mixes the
    /// two.
    fn named_positions(
        names: &[Option<String>],
        class: &str,
        function: &str,
    ) -> Option<Vec<usize>> {
        if names.iter().all(Option::is_none) {
            return None;
        }
        let declared = crate::paramnames::param_names(class, function)?;
        let mut slots: Vec<Option<usize>> = vec![None; names.len()];
        let mut taken = vec![false; declared.len()];
        for (i, name) in names.iter().enumerate() {
            if let Some(n) = name {
                let lower = n.to_ascii_lowercase();
                let at = declared.iter().position(|d| *d == lower)?;
                if taken[at] {
                    return None;
                }
                taken[at] = true;
                slots[i] = Some(at);
            }
        }
        let mut next = 0usize;
        for slot in slots.iter_mut() {
            if slot.is_some() {
                continue;
            }
            while next < taken.len() && taken[next] {
                next += 1;
            }
            if next >= taken.len() {
                return None;
            }
            taken[next] = true;
            *slot = Some(next);
            next += 1;
        }
        slots.into_iter().collect()
    }

    /// Reorder `values` so slot `k` holds the argument the source put there,
    /// filling any skipped slot from the declaration's default.
    ///
    /// A named argument can skip over parameters, and the `.pex` still writes
    /// them: `DisablePlayerControls(abMovement = false, abMenu = false,
    /// abActivate = false, abFighting = true)` writes nine values, five of them
    /// defaults. So the hole is not "no value" — it is the declared default, and
    /// inserting `None` there produced `cast ::temp10 None` into a `bool` slot.
    ///
    /// `None` means the call cannot be placed this way — an unknown parameter
    /// name, a repeated one, or a skipped parameter with no default to fill it —
    /// and the caller keeps the positional reading rather than guessing.
    fn place_named(
        &mut self,
        values: Vec<Value>,
        names: &[Option<String>],
        class: &str,
        function: &str,
        defaults: Option<&'static [Option<(&'static str, &'static str)>]>,
    ) -> Option<Vec<Value>> {
        let slots = Self::named_positions(names, class, function)?;
        let width = slots.iter().copied().max().map(|m| m + 1).unwrap_or(0);
        let mut out: Vec<Option<Value>> = vec![None; width];
        for (v, at) in values.into_iter().zip(slots) {
            out[at] = Some(v);
        }
        let mut filled = Vec::with_capacity(width);
        for (i, slot) in out.into_iter().enumerate() {
            match slot {
                Some(v) => filled.push(v),
                None => {
                    let (kind, text) = defaults?.get(i).copied().flatten()?;
                    filled.push(self.default_value(kind, text)?);
                }
            }
        }
        Some(filled)
    }

    /// An argument list, evaluated in two phases.
    ///
    /// **Phase 1 evaluates every argument, left to right. Phase 2 applies the
    /// slot conversions, left to right.** Nothing is converted while the list is
    /// still being walked.
    ///
    /// Measured on the shipped corpus, and it is not what the obvious
    /// implementation does. `AnimalTrainerSystemScript::BuyAnimal`:
    ///
    /// ```text
    /// Game.GetPlayer().RemoveItem(Gold, AnimalGold.value as int)
    ///   0 callstatic game GetPlayer ::temp0        the receiver
    ///   1 propget    value ::AnimalGold_var ::temp1   argument 1 evaluates
    ///   2 cast       ::temp2 ::temp1                  its own `as int`
    ///   3 cast       ::temp3 ::Gold_var            argument 0's slot cast, LAST
    ///   4 callmethod RemoveItem ::temp0 ::NoneVar ::temp3 ::temp2 …
    /// ```
    ///
    /// `Gold` is an auto property, so argument 0 emits **nothing** of its own —
    /// it needs a `cast` to `Form` and nothing else. Converting it inline puts
    /// that cast at position 1, before argument 1's `propget`, which shifts every
    /// later temporary in the function. Deferring it reproduces the game.
    ///
    /// The order *among* the deferred conversions is left to right, which is
    /// what settles that the list itself is left-to-right — four witnesses from
    /// `examples/argwitness` where every argument is a bare name whose only
    /// instruction is its cast:
    ///
    /// ```text
    /// GhostTrap::onBeginState        cast ::temp3 self    cast ::temp4 hoverDelay
    /// DLC1dunHarkonShadowShield::OnHit cast ::temp8 aggressor  cast ::temp5 weap
    /// atronachFrostActorSCRIPT::…    cast ::temp4 self    cast ::temp5 target
    /// ```
    ///
    /// An explicit `x as T` in the source is part of the expression and stays
    /// where the expression is (`cast ::temp2 ::temp1` above), so this deferral
    /// applies only to the conversions the *parameter's type* asks for.
    fn call_args(
        &mut self,
        buf: &mut FnBuf,
        args: &[Expr],
        names: &[Option<String>],
        line: u16,
        class: &str,
        method: &str,
        defaults: Option<&'static [Option<(&'static str, &'static str)>]>,
        want: impl Fn(usize) -> Option<&'static str>,
    ) -> Result<Vec<Value>, Error> {
        let mut values = Vec::with_capacity(args.len());
        for a in args {
            values.push(self.expr(buf, a, line)?);
        }
        // Named arguments go to their DECLARED slots, so the reordering happens
        // before the conversions — `want(i)` is the type at position `i` of the
        // declaration, and after this the value at `i` is the one that belongs
        // there.
        let mut values = match self.place_named(values.clone(), names, class, method, defaults) {
            Some(placed) => placed,
            // Unplaceable: keep the positional reading, which is what the
            // source looks like when no name was written.
            None => values,
        };
        for (i, v) in values.iter_mut().enumerate() {
            if let Some(w) = want(i) {
                *v = self.coerce_arg(buf, v.clone(), w, line);
            }
        }
        Ok(values)
    }

    /// A value's type when the compiler actually **knows** it, or `None`.
    ///
    /// [`Self::value_type`] has to answer *something* — it feeds opcode choice
    /// and the arithmetic result type — so an identifier with no declaration
    /// falls back to `Int`/`Float`. A conversion must not read that fallback as
    /// knowledge: the CK casts only when it can see a conversion is needed, and
    /// guessing from a made-up type invents casts.
    ///
    /// Measured on `batteringtrap::fireTrap`, `if fireOnlyOnce` where
    /// `fireOnlyOnce` is a `bool` property the type table does not carry: the
    /// game writes `cmp_eq ::temp2 ::fireOnlyOnce_var true` with no cast, and
    /// treating the fallback `Float` as the source type emitted
    /// `cast ::temp2 ::fireOnlyOnce_var` first — which also consumed a slot.
    fn known_type(&self, buf: &FnBuf, v: &Value) -> Option<String> {
        match v {
            Value::Float(_) => Some("Float".to_string()),
            Value::Int(_) => Some("Int".to_string()),
            Value::Bool(_) => Some("Bool".to_string()),
            Value::String(_) => Some("String".to_string()),
            Value::Identifier(i) => {
                if self
                    .strings
                    .get(*i as usize)
                    .map(|s| s.eq_ignore_ascii_case("self"))
                    .unwrap_or(false)
                {
                    return Some(self.script.name.clone());
                }
                self.declared_type(buf, *i)
            }
            _ => None,
        }
    }

    /// The declared type of a name used as a receiver, looking through the
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

    /// The declared class of a receiver **expression**, not just a name.
    ///
    /// A property write can be on a cast — `(GetOwningQuest() as SomeScript).Prop
    /// = v` — and resolving only bare names left the property untyped, so its
    /// value was never converted and the cast the game emits was missing.
    fn receiver_class_of(&self, buf: &FnBuf, e: &Expr) -> Option<String> {
        match e {
            Expr::Ident(n) => self.receiver_class(buf, n),
            Expr::Cast(_, ty) => Some(ty.0.clone()),
            Expr::Call { callee, .. } => match callee.as_ref() {
                Expr::Member(obj, m) => {
                    let c = self.receiver_class_of(buf, obj)?;
                    crate::signatures::return_type(&c, m).map(str::to_string)
                }
                Expr::Ident(n) => self.call_result_type(n).to_string().into(),
                _ => None,
            },
            // A **chained** member: `kmyquest.CWs.WhiterunLocation`. The
            // receiver's class is the type of the property read before it, so
            // `CWScript.CWs` is a `CWScript` and the property asked of it is
            // `cwscript.whiterunlocation`. Without this arm the read fell through
            // to `var`, and `cityVar == kmyquest.CWs.WhiterunLocation` (cityVar a
            // `location`) needed an extra cast — then took a different temporary,
            // displacing the rest of a 900-line quest fragment.
            Expr::Member(obj, name) => {
                let c = self.receiver_class_of(buf, obj)?;
                crate::signatures::property_type(&c, name).map(str::to_string)
            }
            _ => None,
        }
    }

    /// Whether `name` is an `auto` property, whose storage is a backing variable.
    ///
    /// `AutoReadOnly` is excluded: it has no variable, because its value is a
    /// constant folded into a generated getter.
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
                // `+` with a String on either side is CONCATENATION, not
                // addition, and it is a two-instruction shape: the left operand
                // is cast into the result slot, then `strcat` appends the right
                // to it in place.
                //   trace(self + " Opening")
                //   -> cast ::temp2 self
                //      strcat ::temp2 ::temp2 " Opening"
                // Choosing `fadd` there produced `fadd ::temp7 self " Opening"`,
                // which is not a program the VM can run. Witness:
                // BlackreachDoorScript.SetOpen.
                if op == "+" {
                    let lt = self.value_type(buf, &l);
                    let rt = self.value_type(buf, &r);
                    let ls = lt.eq_ignore_ascii_case("string");
                    let rs = rt.eq_ignore_ascii_case("string");
                    if ls || rs {
                        // `strcat <dest> <left> <right>` takes the operands in
                        // SOURCE order, and whichever side is not already a
                        // String is **cast into the destination** first:
                        //
                        //   "a(" + x      -> cast ::temp104 x
                        //                    strcat ::temp104 "a(" ::temp104
                        //   self + " b"   -> cast ::temp2 self
                        //                    strcat ::temp2 ::temp2 " b"
                        //
                        // A chain reuses the destination, which is why
                        // `"a(" + x + ")"` emits a second
                        // `strcat ::temp104 ::temp104 ")"` rather than a fresh
                        // slot. Casting the LEFT operand unconditionally put the
                        // cast on the literal in the first shape, which is a
                        // different program. Witness: CWScript.IsPlayerInMyFaction.
                        // Which slot the result takes. Measured on
                        // `CWScript.setOwnerFalkreath`, a chain of five:
                        //
                        //   0 strcat ::temp363 "CW …" "FactionToOwn: "
                        //   1 cast   ::temp364 FactionToOwn
                        //   2 strcat ::temp364 ::temp363 ::temp364
                        //   3 strcat ::temp363 ::temp364 ", DiplomaticVictory: "
                        //   4 cast   ::temp364 DiplomaticVictory
                        //   5 strcat ::temp364 ::temp363 ::temp364
                        //
                        // So the destination is the CAST's slot when exactly one
                        // side needs converting (`strcat ::temp364 ::temp363
                        // ::temp364` — destination and right operand are the same
                        // slot), and a slot off the free list when neither does
                        // (step 3 writes `::temp363`, which step 2 had freed).
                        //
                        // Reusing the LEFT operand unconditionally, which is what
                        // this did, puts every step of a chain in one slot and
                        // shifts everything after it. The left operand is still
                        // live while the destination is chosen, so it is released
                        // afterwards — and never when it IS the destination.
                        let mut lv = l.clone();
                        let mut rv = r.clone();
                        let mut dest: Option<Value> = None;
                        if !ls {
                            let t = Value::Identifier(self.temp(buf, "String"));
                            let mut ins = Instruction::new(opcode_of("cast"));
                            ins.args.push(t.clone());
                            ins.args.push(l.clone());
                            buf.push(ins, line);
                            self.release_temp(buf, &l);
                            lv = t.clone();
                            dest = Some(t);
                        }
                        if !rs {
                            let t = Value::Identifier(self.temp(buf, "String"));
                            let mut ins = Instruction::new(opcode_of("cast"));
                            ins.args.push(t.clone());
                            ins.args.push(r.clone());
                            buf.push(ins, line);
                            self.release_temp(buf, &r);
                            rv = t.clone();
                            if dest.is_none() {
                                dest = Some(t);
                            }
                        }
                        let dest = dest.unwrap_or_else(|| Value::Identifier(self.temp(buf, "String")));
                        let mut ins = Instruction::new(opcode_of("strcat"));
                        ins.args.push(dest.clone());
                        ins.args.push(lv.clone());
                        ins.args.push(rv.clone());
                        buf.push(ins, line);
                        // The operands are dead; the destination is not, and in
                        // the converted shapes it IS one of them.
                        if lv != dest {
                            self.release_temp(buf, &lv);
                        }
                        if rv != dest {
                            self.release_temp(buf, &rv);
                        }
                        return Ok(dest);
                    }
                }
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
                // A comparison or a numeric operation converts an operand only
                // when both sides' types are **known**. `value_type` has to
                // answer something and falls back to `Int`/`Float`, so an
                // untyped operand (a property the tables do not carry) used to
                // look numeric and got a `cast` the CK does not emit:
                // `if fireOnlyOnce` in `batteringtrap::fireTrap` is
                // `cmp_eq ::temp2 ::fireOnlyOnce_var true`, and casting the
                // variable to Bool first both invented an instruction and took a
                // slot.
                let known = self.known_type(buf, &l).zip(self.known_type(buf, &r));
                if (is_cmp || (is_arith && numeric_pair)) && known.is_some() {
                    let (lt, rt) = known.unwrap();
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
                        // A `None` operand casts from the **None literal**, not
                        // from the `::NoneVar` identifier. `GetReference() != None`
                        // compiles to `cast ::temp85 None` / `cmp_eq ::temp86
                        // ::temp84 ::temp85`; pushing the identifier instead is a
                        // different instruction with the same intent.
                        let src = match &src {
                            Value::Identifier(i)
                                if self
                                    .strings
                                    .get(*i as usize)
                                    .map(|s| s.eq_ignore_ascii_case("::NoneVar"))
                                    .unwrap_or(false) =>
                            {
                                Value::None
                            }
                            other => other.clone(),
                        };
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
                // A minus applied **directly to a numeric literal** is a negative
                // literal, not a negation instruction. Measured,
                // `ArenaScript::PickNextFight`:
                //
                //   source: minFight = -1
                //   game:   assign minFight -1
                //   ours:   ineg ::temp39 1        <- one instruction too many
                //           assign minFight ::temp39
                //
                // The extra instruction displaces every later temporary in the
                // function, which is why the buckets it produced read as
                // `ineg vs assign`, `ineg vs callmethod`, `ineg vs cmp_eq` and
                // so on: the game's stream at that index is whatever legitimately
                // follows. Folding costs nothing and is what the compiler writes.
                match (op.as_str(), &v) {
                    ("-", Value::Int(n)) => return Ok(Value::Int(-*n)),
                    ("-", Value::Float(f)) => return Ok(Value::Float(-*f)),
                    _ => {}
                }
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
                        // operand's type decides, and it is the DECLARED type —
                        // not merely "is this a float literal". Measured on
                        // `CWArrowVolleyParentScript.FireSelfAndLinkedRefs`,
                        // whose `AimDevianceVertical` is a `float property`:
                        //   game:  fneg ::temp10 ::AimDevianceVertical_var
                        //   ours:  ineg ::temp10 ::AimDevianceVertical_var
                        // An untyped operand falls back to int, which is the
                        // safer reading: a wrong `fneg` on an int is the same
                        // class of error in the other direction, and `ineg` on a
                        // `var` is what the corpus shows for an unresolvable name.
                        match &v {
                            Value::Float(_) => opcode_of("fneg"),
                            _ => match self.known_type(buf, &v) {
                                Some(t) if t.eq_ignore_ascii_case("float") => opcode_of("fneg"),
                                _ => opcode_of("ineg"),
                            },
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
            Expr::Call { callee, args, names } => {
                // Three shapes, and which one it is decides the opcode:
                //   `f(...)`            — callmethod on self
                //   `a.f(...)`          — callmethod on `a`
                //   `Class.f(...)`      — callstatic
                match callee.as_ref() {
                    Expr::Ident(name) => {
                        // `New Bool[10]` allocates an array: `array_create dest
                        // size`, with **no** receiver and no method. The parser
                        // spells it as a call on `New <Type>`, which the method
                        // path turned into `callmethod New ObjectReference self
                        // ::temp0 20` — a call to a function that does not exist.
                        // Measured on `cchousequestscript::OnInit`, whose
                        // `New ObjectReference[20]` is the game's
                        // `array_create ::temp0 20`.
                        if let Some(base) = name.strip_prefix("New ") {
                            let size = match args.first() {
                                Some(e) => self.expr(buf, e, line)?,
                                None => Value::Int(0),
                            };
                            let ty = format!("{}[]", base);
                            let tmp = self.temp(buf, &ty);
                            let mut ins = Instruction::new(opcode_of("array_create"));
                            ins.args.push(Value::Identifier(tmp));
                            ins.args.push(size);
                            buf.push(ins, line);
                            return Ok(Value::Identifier(tmp));
                        }
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
                            // The argument list, evaluated left to right, with
                            // each argument's slot conversion after the list —
                            // see `call_args`. Positions in the vararg list stay
                            // in source order either way.
                            let va = self.call_args(buf, args, names, line, &own, name, defs, |i| {
                                crate::signatures::arg_type(&own, name, i)
                            })?;
                            ins.varargs = self.pad_defaults(va, defs, name);
                            let released: Vec<Value> = ins.varargs.clone();
                            buf.push(ins, line);
                            // The arguments are consumed by the call and are dead.
                            // Every call path must do this: a path that forgets
                            // keeps its temporaries live for the rest of the
                            // function and shifts every later allocation.
                            for a in &released {
                                self.release_temp(buf, a);
                            }
                            return Ok(Value::Identifier(tmp));
                        }
                        // A bare call resolves against a `global` function
                        // BEFORE it resolves as a method on `self`. `Game`,
                        // `Utility`, `Math` and `Debug` are in scope in every
                        // script without an `Import`, and so are the globals of
                        // any other class the corpus declares one on:
                        //   wait (RandomFloat (DelayMin, DelayMax))
                        //   -> callstatic utility RandomFloat ::temp3 …
                        //   -> callstatic utility wait ::NoneVar ::temp3
                        //   log("CWScript", "…")            in CWScript
                        //   -> callstatic CWScript log ::NoneVar …
                        //   StopInstance(instance)          anywhere
                        //   -> callstatic sound stopinstance ::NoneVar …
                        // Emitting `callmethod f self` is accepted by the loader
                        // and fails at run time, because `self` has no such
                        // member. The script's own declarations win.
                        //
                        // `globals::GLOBALS` is the whole set, measured: over the
                        // corpus no function is `global` on more than one class,
                        // so the name alone decides the class. It supersedes
                        // `signatures::imported_class`, which covers only the
                        // four always-imported classes and so missed
                        // `CWScript.log`, `Sound.StopInstance`,
                        // `Weather.ReleaseOverride`, `ImageSpaceModifier.
                        // RemoveCrossfade`, `Message.ResetHelpMessage` and the
                        // rest — 455 call sites that were emitted as
                        // `callmethod` on a receiver with no such member.
                        if let Some(cls) = crate::globals::global_class(name)
                            .or_else(|| crate::signatures::imported_class(name))
                        {
                            // A `global` on the script's own class is a
                            // `callstatic` even though the script declares it —
                            // `CWScript.log` is the witness. Elsewhere the
                            // script's own declaration wins, because it shadows
                            // the imported name.
                            if self.class_is_self_or_ancestor(cls)
                                || !self.declares_own_function(name)
                            {
                                let target = self.intern(cls);
                                let ty = self.static_result_type(cls, name);
                                let defs = crate::signatures::method_signature(cls, name)
                                    .map(|(_, d)| d)
                                    .or_else(|| crate::signatures::defaults(name));
                                let va = self.call_args(buf, args, names, line, cls, name, defs, |i| {
                                    crate::signatures::arg_type(cls, name, i)
                                })?;
                                // Destination after the arguments — see the
                                // member-call path.
                                let tmp = self.temp(buf, ty);
                                let mut ins = Instruction::new(opcode_of("callstatic"));
                                ins.args.push(Value::Identifier(target));
                                ins.args.push(Value::Identifier(fname));
                                ins.args.push(Value::Identifier(tmp));
                                ins.varargs = self.pad_defaults(va, defs, name);
                                let released: Vec<Value> = ins.varargs.clone();
                                buf.push(ins, line);
                                for a in &released {
                                    self.release_temp(buf, a);
                                }
                                return Ok(Value::Identifier(tmp));
                            }
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
                        let mut ins = Instruction::new(opcode_of("callmethod"));
                        ins.args.push(Value::Identifier(fname));
                        ins.args.push(Value::Identifier(self_i));
                        // A bare `f(...)` is a call on `self`, and its arguments
                        // are converted exactly like a member call's. This path
                        // had padding but no coercion, so `SetForcedLandingMarker(None)`
                        // passed `::NoneVar` where the game emits
                        // `cast ::temp7 None` first — the difference in ~2,290
                        // functions, because `None` is how Papyrus spells an
                        // unset object.
                        let own = self.own_class().unwrap_or_default();
                        // A bare `f(...)` evaluates its arguments left to right
                        // like every other path — the earlier note that it
                        // differed was a fitted artifact. See `call_args`.
                        // The defaults come from `self`'s class chain, not from the
                        // unqualified table: `SetRestrained()` on an `Actor` pads
                        // `true` from `actor.psc`, and the unqualified table has no
                        // default for the name at all.
                        let defs = self
                            .method_defaults_in_chain(name)
                            .or_else(|| crate::signatures::defaults(name));
                        let dealt = self.call_args(buf, args, names, line, &own, name, defs, |i| {
                            crate::signatures::arg_type(&own, name, i)
                        })?;
                        let dealt = self.pad_defaults(dealt, defs, name);
                        let released: Vec<Value> = dealt.clone();
                        // Destination after the arguments — see the member-call
                        // path.
                        let tmp = self.temp(buf, ty);
                        ins.args.push(Value::Identifier(tmp));
                        ins.varargs = dealt;
                        buf.push(ins, line);
                        // A temporary consumed as an argument is released once
                        // the call is emitted. The member-call path did this and
                        // the bare-call path did not, so every temporary passed
                        // to `f(...)` stayed live for the rest of the function:
                        // `activate(self, true)` in
                        // `ActivateAgainAfterDelay.OnActivate` left ::temp0
                        // occupied and the later `cast ::temp0 self` became
                        // `cast ::temp4 self`, displacing everything after it.
                        for a in &released {
                            self.release_temp(buf, a);
                        }
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
                                // Left to right, like every other call path.
                                let mut va = Vec::with_capacity(args.len());
                                for a in args {
                                    va.push(self.expr(buf, a, line)?);
                                }
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
                                let va = self.pad_defaults(va, defs, method);
                                let released: Vec<Value> = va.clone();
                                // Destination after the arguments — see the
                                // member-call path.
                                let tmp = self.temp(buf, ty);
                                let mut ins = Instruction::new(opcode_of("callparent"));
                                ins.args.push(Value::Identifier(m));
                                ins.args.push(Value::Identifier(tmp));
                                ins.varargs = va;
                                buf.push(ins, line);
                                for a in &released {
                                    self.release_temp(buf, a);
                                }
                                return Ok(Value::Identifier(tmp));
                            }
                        }
                        if let Expr::Ident(class) = obj.as_ref() {
                            if let Some(target) = self
                                .static_target(class)
                                .or_else(|| self.class_global_target(buf, class))
                            {
                                let m = self.intern(method);
                                // The result's type decides the temporary, and
                                // that decides whether a later comparison needs
                                // a `cast`. Measured, not guessed.
                                let ty = self.static_result_type(class, method);
                                let defs = crate::signatures::defaults(method);
                                let cls = class.to_ascii_lowercase();
                                let sig = crate::signatures::method_params(&cls, method);
                                let va = self.call_args(buf, args, names, line, &cls, method, defs, |i| {
                                    sig.and_then(|s| s.get(i).copied())
                                })?;
                                // Destination after the arguments — see the
                                // member-call path.
                                let tmp = self.temp(buf, ty);
                                let mut ins = Instruction::new(opcode_of("callstatic"));
                                ins.args.push(Value::Identifier(target));
                                ins.args.push(Value::Identifier(m));
                                ins.args.push(Value::Identifier(tmp));
                                ins.varargs = self.pad_defaults(va, defs, method);
                                let released: Vec<Value> = ins.varargs.clone();
                                buf.push(ins, line);
                                for a in &released {
                                    self.release_temp(buf, a);
                                }
                                return Ok(Value::Identifier(tmp));
                            }
                        }
                        // The receiver's declared type tells us which class the
                        // method belongs to, so the corpus table can supply the
                        // result type. Without it the temporary is `var`, and
                        // the corpus shows `var` for only a handful of the
                        // 4,000+ call results it records.
                        // The receiver's class, for a CALL as well as a name:
                        // `FragmentSack.GetReference().GetParentCell().Reset()`
                        // has a receiver whose type is `cell`, and without it the
                        // padding fell back to the unqualified `reset` table and
                        // appended a `None` the game does not pass. Resolving it
                        // also gives chained calls their result type.
                        let recv_class = self.receiver_class_of(buf, obj);
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
                        // The destination is allocated AFTER the arguments: the
                        // argument's own `cast` takes the lower slot. Measured on
                        // `DLC1ChaurusCocoonScript.Ready.onEndState`, whose
                        // `Self.Activate(Self)` is
                        //   cast ::temp0 self
                        //   callmethod Activate self ::temp1 ::temp0 false
                        // — allocating the destination first gave
                        // `cast ::temp1 self` / `callmethod … ::temp0 ::temp1`,
                        // the same instructions with the two slots exchanged.
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
                        // The argument list, left to right, with each argument's
                        // slot conversion emitted after the list — `call_args`.
                        // Converting inline put a bare property's cast before the
                        // next argument's `propget`, shifting every later
                        // temporary: `AnimalTrainerSystemScript::BuyAnimal`.
                        let va = self.call_args(buf, args, names, line, class, method, defs, |i| {
                            crate::signatures::arg_type(class, method, i)
                        })?;
                        // Omitted trailing arguments are written out, not
                        // dropped: the .pex must carry the count the game has.
                        let va = self.pad_defaults(va, defs, method);
                        let released: Vec<Value> = va.clone();
                        // The destination is allocated AFTER the argument list,
                        // so an argument's own `cast` takes the lower slot. See
                        // the `DLC1ChaurusCocoonScript` measurement above.
                        let tmp = self.temp(buf, ty);
                        let mut ins = Instruction::new(opcode_of("callmethod"));
                        ins.args.push(Value::Identifier(m));
                        ins.args.push(recv.clone());
                        ins.args.push(Value::Identifier(tmp));
                        ins.varargs = va;
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
                // `.Length` on an array is `array_length`, not a property read:
                //   array_length ::temp73 ::LogPiles_var
                // Emitting `propget Length ::LogPiles_var ::temp86` asks the VM
                // for a member that does not exist. `Length` is only ever the
                // array pseudo-property, so the name alone settles it; the type
                // is consulted only to avoid stealing a real property called
                // `Length` from an object receiver.
                if name.eq_ignore_ascii_case("length") {
                    let ty = match obj.as_ref() {
                        Expr::Ident(n) => self.receiver_class(buf, n),
                        Expr::Cast(_, t) => Some(t.0.clone()),
                        _ => None,
                    };
                    if ty.as_deref().map(|t| t.ends_with("[]")).unwrap_or(true) {
                        let a = self.expr(buf, obj, line)?;
                        let dest = self.temp(buf, "Int");
                        let mut ins = Instruction::new(opcode_of("array_length"));
                        ins.args.push(Value::Identifier(dest));
                        ins.args.push(a);
                        buf.push(ins, line);
                        return Ok(Value::Identifier(dest));
                    }
                }
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
                    // A CHAINED receiver — `kmyquest.CWs.WhiterunLocation` — is
                    // the case the `Ident` arm cannot reach: the property asked
                    // of `kmyquest.CWs` is `cwscript.whiterunlocation`, and
                    // `CWScript` is what the first read's type says. Without this
                    // the read typed `var` and the comparison against a
                    // `location` variable gained a cast.
                    Expr::Member(..) | Expr::Call { .. } | Expr::Cast(..) => self
                        .receiver_class_of(buf, obj)
                        .and_then(|cls| crate::signatures::property_type(&cls, name))
                        .map(slot_type)
                        .unwrap_or_else(|| self.property_type(name)),
                    Expr::Ident(n) => {
                        // The receiver may be a LOCAL (`ArenaScript aScript`,
                        // declared inside the function) as well as a script-level
                        // variable. Asking only the script-level table left
                        // `aScript.NextWagerFight` untyped, so the value it
                        // produced could not be converted into the `quest` slot it
                        // was assigned to and the cast the game emits was missing.
                        let cls = self.receiver_class(buf, n).unwrap_or_default();
                        // The corpus table keeps the `.psc` spelling of a class;
                        // a slot stores it lowercase. Normalise both sources the
                        // same way, or the destination is typed `Location` where
                        // the corpus writes `location`.
                        crate::signatures::property_type(&cls, name)
                            .map(slot_type)
                            .unwrap_or_else(|| self.property_type(name))
                    }
                    _ => self.property_type(name),
                };
                let tmp = self.temp(buf, &pty);
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
                // The element's type is the **array's** element type, and it
                // decides both the slot's type and whether the value converts on
                // the way into wherever it goes. Measured on
                // `BYOHHouseBuildingScript.InitializeDLC`, whose
                // `CrimeFactions` is a `Faction[]`: the game types the read
                // `::temp60:faction` and casts it to `form` for `AddForm`, while
                // a `var` slot could not convert at all and the call lost its
                // `cast` — then every later temporary with it.
                let elem = match arr.as_ref() {
                    Expr::Ident(n) => self
                        .type_of(n)
                        .map(|t| t.base())
                        .unwrap_or_else(|| "var".to_string()),
                    _ => "var".to_string(),
                };
                let a = self.expr(buf, arr, line)?;
                let i = self.expr(buf, idx, line)?;
                let tmp = self.temp(buf, &elem);
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
    /// Whether the script declares a function of this name in any state.
    ///
    /// Needed because a script's OWN function shadows an implicitly-imported
    /// engine function of the same name: a script that declares `abs(...)`
    /// calls its own, not `Math.Abs`.
    ///
    /// A `global` declaration on the script's own class is the exception — see
    /// [`Self::class_is_self_or_ancestor`].
    fn declares_own_function(&self, name: &str) -> bool {
        fn walk(d: &crate::parse::DeclKind, want: &str) -> bool {
            match d {
                crate::parse::DeclKind::Function(f) => f.name.eq_ignore_ascii_case(want),
                crate::parse::DeclKind::State(s) => s
                    .functions
                    .iter()
                    .any(|f| f.name.eq_ignore_ascii_case(want)),
                _ => false,
            }
        }
        self.script.decls.iter().any(|d| walk(&d.kind, name))
    }

    /// Whether `class` is this script's own class or one of its ancestors.
    ///
    /// Needed because a `global` function declared on the script's **own** class
    /// is a `callstatic` on that class even though the script declares it:
    /// `CWScript` declares `Function log(...) Global` and calls it bare, and the
    /// game writes `callstatic CWScript log`. Treating "the script declares this
    /// name" as "it is a method on self" emitted `callmethod log self`, a member
    /// the VM has no slot for.
    /// The parameter defaults of `method`, found by walking the script's own
    /// class and then its ancestors.
    ///
    /// A bare call `f(...)` is a call on `self`, so the defaults come from
    /// `self`'s class — not from the **unqualified** table, which has one entry
    /// per name across the whole corpus and is therefore often a different
    /// declaration. Measured on `CWFortSoldierWoundedScript.OnCellLoad`, whose
    /// `SetRestrained()` is a call on an `Actor`:
    ///
    ///   source:  SetRestrained()
    ///   game:    callmethod SetRestrained self ::NoneVar true
    ///   ours:    callmethod SetRestrained self ::NoneVar
    ///
    /// `actor.psc` declares `SetRestrained(bool abRestrained = true)`, so the
    /// default is `true` and the call is one operand short. The unqualified
    /// table carries no default for the name at all.
    fn method_defaults_in_chain(
        &self,
        method: &str,
    ) -> Option<&'static [Option<(&'static str, &'static str)>]> {
        let mut here = Some(self.script.name.to_ascii_lowercase());
        for _ in 0..12 {
            let Some(c) = here else { break };
            if let Some(d) = crate::signatures::method_defaults(&c, method) {
                return Some(d);
            }
            here = crate::signatures::parent(&c).map(str::to_string);
        }
        None
    }

    fn class_is_self_or_ancestor(&self, class: &str) -> bool {
        if self.script.name.eq_ignore_ascii_case(class) {
            return true;
        }
        let mut here = self
            .script
            .parent
            .clone()
            .or_else(|| Some(self.symbols.parent.clone()).filter(|p| !p.is_empty()));
        let mut hops = 0;
        while let Some(c) = here {
            if c.eq_ignore_ascii_case(class) {
                return true;
            }
            if hops > 12 {
                break;
            }
            hops += 1;
            here = crate::signatures::parent(&c).map(str::to_string);
        }
        false
    }

    /// The interned name of a class whose functions can be called statically,
    /// or `None`.
    ///
    /// [`Self::static_target`] covers the four classes that are in scope in
    /// every script. This adds the rest: a class that declares `global`
    /// functions is a receiver you can call on by class name —
    /// `Sound.StopInstance(x)` compiles to `callstatic sound StopInstance`, and
    /// reading `Sound` as a variable emits `callmethod StopInstance sound`, a
    /// call on a member the VM has no slot for. Measured on
    /// `CWSiegeScript::StopCombatSoundsLoop`.
    ///
    /// A name that is a parameter, a local or a declared property is a VALUE
    /// however it is spelled, so it is never a class here: `Sound Property sfx
    /// auto` makes `sfx.Play(...)` a call on the instance, and an earlier
    /// version treated exactly that as `callstatic` and ran against the wrong
    /// receiver.
    fn class_global_target(&mut self, buf: &FnBuf, name: &str) -> Option<StrIndex> {
        if !crate::globals::is_global_class(name) {
            return None;
        }
        let lower = name.to_ascii_lowercase();
        if buf
            .params
            .iter()
            .chain(&buf.declared)
            .any(|(p, _)| *p == lower)
        {
            return None;
        }
        if self.type_of(name).is_some() {
            return None;
        }
        // `self` is the script's own object, never a class.
        if lower == "self" {
            return None;
        }
        Some(self.intern(&lower))
    }

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

/// A declared type name as a **locals-table** slot type.
///
/// The four primitives are capitalised in a slot and a class name is lowercase:
/// measured over the corpus, a class return type is capitalised in 297 cases and
/// lowercase in 89, while `::X_var` slots are lowercase 91% of the time. The
/// corpus tables in `signatures.rs` keep the `.psc` spelling, so anything read
/// from them has to pass through here before it becomes a slot — otherwise a
/// `propget` destination is typed `Location` where the file writes `location`,
/// and the slot type disagreeing with the corpus shows up as a `cast` that should
/// not be there.
fn slot_type(declared: &str) -> String {
    match declared.trim_end_matches("[]") {
        t if t.eq_ignore_ascii_case("int") => "Int".to_string(),
        t if t.eq_ignore_ascii_case("float") => "Float".to_string(),
        t if t.eq_ignore_ascii_case("bool") => "Bool".to_string(),
        t if t.eq_ignore_ascii_case("string") => "String".to_string(),
        t if t.eq_ignore_ascii_case("none") => "var".to_string(),
        t => t.to_ascii_lowercase(),
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

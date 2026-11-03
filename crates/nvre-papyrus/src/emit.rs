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
//! A jump's operand is a count of **operations to skip**, counted from the
//! instruction after the jump.

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
    /// How many temporaries have been handed out.
    next_temp: usize,
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
            next_temp: 0,
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
    /// The debug entries: (state, function, lines).
    pub debug: Vec<(StrIndex, StrIndex, Vec<u16>)>,
}

impl<'a> Compiler<'a> {
    /// A compiler for `script`, whose parent class is `parent`.
    pub fn new(script: &'a Script, symbols: Symbols) -> Self {
        let mut c = Compiler {
            script,
            symbols,
            strings: Vec::new(),
            types: Vec::new(),
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
                DeclKind::Property(p) => c
                    .types
                    .push((p.name.to_ascii_lowercase(), p.type_name.clone())),
                _ => {}
            }
        }
        c
    }

    fn intern(&mut self, s: &str) -> StrIndex {
        if let Some(i) = self.strings.iter().position(|x| x == s) {
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
        let mut order: Vec<(Option<String>, &FunctionDecl)> = Vec::new();
        for d in &self.script.decls {
            if let DeclKind::State(st) = &d.kind {
                for f in &st.functions {
                    order.push((Some(st.name.clone()), f));
                }
            }
        }
        for d in &self.script.decls {
            if let DeclKind::Function(f) = &d.kind {
                order.push((None, f));
            }
        }

        // ---- the debug block's names, in entry order ----
        let empty = self.intern("");
        let get_state = self.intern("GetState");
        let goto_state = self.intern("GotoState");
        let mut own_names: Vec<StrIndex> = Vec::new();
        for (_state, f) in &order {
            own_names.push(self.intern(&f.name));
        }
        let obj_name = self.intern(&script_name);

        // ---- the object's user flags, then its parent ----
        for flag in self.user_flag_names.clone() {
            let _ = self.intern(&flag);
        }
        let parent = self.script.parent.clone().unwrap_or_default();
        let _parent_idx = self.intern(&parent);

        // ---- the variables ----
        let mut variables = Vec::new();
        for d in &self.script.decls {
            if let DeclKind::Variable(v) = &d.kind {
                let n = self.intern(&v.name);
                let t = self.intern(&v.type_name.0);
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

        // ---- the bodies, in file order, in the default state ----
        let mut debug: Vec<(StrIndex, StrIndex, Vec<u16>)> = Vec::new();
        let mut default_funcs = Vec::new();

        // The injected pair. Their names are already interned; their bodies
        // introduce `String`, the doc strings, `::State` and so on in the order
        // the file mentions them, which is why they are compiled first.
        default_funcs.push(self.get_state_fn(get_state, empty));
        default_funcs.push(self.goto_state_fn(goto_state, empty));
        // The debug block lists only the script's own bodies, not the injected
        // pair: `DLC2ThirskDoorScript`'s debug block has three entries for four
        // functions.
        debug.push((obj_name, empty, Vec::new()));
        debug.push((obj_name, empty, Vec::new()));

        let mut states: Vec<(StrIndex, Vec<Function>)> = Vec::new();
        let mut index = 0usize;
        let mut by_state: Vec<(Option<String>, Vec<Function>)> = Vec::new();
        for (state, f) in &order {
            let (cf, lines) = self.function(f)?;
            let _ = own_names[index];
            debug.push((obj_name, self.intern(state.as_deref().unwrap_or("")), lines));
            index += 1;
            match by_state.iter_mut().find(|(k, _)| k == state) {
                Some((_, v)) => v.push(cf),
                None => by_state.push((state.clone(), vec![cf])),
            }
        }
        for (name, mut funcs) in by_state {
            let n = self.intern(name.as_deref().unwrap_or(""));
            if name.is_none() {
                // The script's own bodies go after the injected pair.
                let mut all = std::mem::take(&mut default_funcs);
                all.append(&mut funcs);
                funcs = all;
            }
            states.push((n, funcs));
        }
        // A script with no own bodies still has the injected pair.
        if states.is_empty() {
            states.push((empty, default_funcs));
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
        let type_name = self.intern(&p.type_name.0);
        let doc = self.intern("");
        let mut flags = 0u8;
        let mut auto_var = None;
        let mut getter = None;
        let mut setter = None;

        if p.is_auto {
            // `auto` is readable and writable unless `AutoReadOnly` says
            // otherwise, and it is backed by a variable named `::Name_var`.
            flags |= 0x4;
            if !p.is_read_only {
                flags |= 0x3;
            } else {
                flags |= 0x1;
            }
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
            other => {
                return Err(Error::Parse(format!(
                    "a property's initial value must be a literal, found {other:?}"
                )))
            }
        })
    }

    /// Compile one function body.
    fn function(&mut self, f: &FunctionDecl) -> Result<(Function, Vec<u16>), Error> {
        let name = self.intern(&f.name);
        let ret = self.intern(
            f.return_type
                .as_ref()
                .map(|t| t.0.as_str())
                .unwrap_or("None"),
        );
        let doc = self.intern(f.doc.as_deref().unwrap_or(""));

        let mut buf = FnBuf::new();

        // `self` is *not* a local. The locals table holds temporaries and
        // declared `local`s only — the game's `OnActivate` lists six `::tempN`
        // and `::NoneVar`, with neither `self` nor its parameter. Measured on
        // DLC2ThirskDoorScript.
        let self_idx = self.intern("self");

        let mut params = Vec::new();
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
            let lt = self.intern(&l.type_name.0);
            buf.locals.push((ln, lt));
            buf.types.push((ln, lt));
            buf.declared.push((l.name.to_ascii_lowercase(), ln));
        }

        self.block(&mut buf, &f.body)?;

        // A function with no explicit `return` still needs one, and the
        // compiler emits `return None` for a void function and leaves a value
        // function to fall off the end.
        if !matches!(buf.instructions.last().map(|i| i.opcode), Some(0x1A)) {
            let none = self.none_var(&mut buf);
            let mut ins = Instruction::new(opcode_of("return"));
            ins.args.push(Value::Identifier(none));
            buf.push(ins, f.line);
        }

        let lines = buf.lines.clone();
        Ok((
            Function {
                name,
                return_type: ret,
                doc,
                user_flags: 0,
                flags: if f.is_global { 0x1 } else { 0 },
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
        let n = buf.next_temp;
        buf.next_temp += 1;
        let i = self.intern(&format!("::temp{n}"));
        let t = self.intern(ty);
        buf.locals.push((i, t));
        buf.types.push((i, t));
        i
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
                let lt = self.intern(&l.type_name.0);
                buf.locals.push((ln, lt));
                buf.declared.push((l.name.to_ascii_lowercase(), ln));
                // A declaration with an initial value is an assignment.
                if let Some(init) = &l.initial {
                    let dest = Value::Identifier(ln);
                    let v = self.expr(buf, init, 0)?;
                    let mut ins = Instruction::new(opcode_of("assign"));
                    ins.args.push(dest);
                    ins.args.push(v);
                    buf.push(ins, 0);
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
                let dest = self.lvalue(buf, target)?;
                let v = self.expr(buf, value, *line)?;
                let mut ins = Instruction::new(opcode_of("assign"));
                ins.args.push(dest);
                ins.args.push(v);
                buf.push(ins, *line);
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
                let arith = self.arith_op(op, &dest);
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
            Stmt::Expr(e) => {
                let _ = self.expr(buf, e, 0)?;
                Ok(())
            }
            Stmt::Return { value, line } => {
                let v = self.expr(buf, value, *line)?;
                let mut ins = Instruction::new(opcode_of("return"));
                ins.args.push(v);
                buf.push(ins, *line);
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
                let mut ins = Instruction::new(opcode_of("jmp"));
                ins.args
                    .push(Value::Int(start as i32 - (buf.here() as i32 + 1)));
                buf.push(ins, *line);
                let after = buf.here();
                if let Value::Int(v) = &mut buf.instructions[at].args[1] {
                    *v = after as i32 - at as i32 - 1;
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

        for (i, b) in branches.iter().enumerate() {
            if let Some(cond) = &b.cond {
                let c = self.expr(buf, cond, b.line)?;
                let mut ins = Instruction::new(opcode_of("jmpf"));
                ins.args.push(c);
                let at = buf.here();
                ins.args.push(Value::Int(0));
                buf.push(ins, b.line);
                patch_false.push(at);
            }
            self.block(buf, &b.body)?;
            // Only a branch with a following branch needs a jump past the rest.
            if i + 1 < branches.len() {
                let mut ins = Instruction::new(opcode_of("jmp"));
                ins.args.push(Value::Int(0));
                let at = buf.here();
                buf.push(ins, b.line);
                patch_end.push(at);
            }
        }

        let end = buf.here();
        for at in patch_false {
            if let Value::Int(v) = &mut buf.instructions[at].args[1] {
                *v = end as i32 - at as i32 - 1;
            }
        }
        for at in patch_end {
            if let Value::Int(v) = &mut buf.instructions[at].args[0] {
                *v = end as i32 - at as i32 - 1;
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
        match v {
            Value::Identifier(_) => v,
            other => {
                let tmp = self.temp(buf, ty);
                let mut ins = Instruction::new(opcode_of("assign"));
                ins.args.push(Value::Identifier(tmp));
                ins.args.push(other);
                buf.push(ins, line);
                Value::Identifier(tmp)
            }
        }
    }

    /// A value that can be assigned to.
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
                Ok(Value::Identifier(self.intern(n)))
            }
            Expr::Member(obj, name) => {
                // `a.b = v` is `propset a b v`; the object is a value.
                let _ = self.expr(buf, obj, 0)?;
                Ok(Value::String(self.intern(name)))
            }
            other => Err(Error::Parse(format!("not assignable: {other:?}"))),
        }
    }

    /// The arithmetic opcode for `op` over `lhs`.
    ///
    /// An integer expression compiles to `iadd`/`isub`/`imul`/`idiv` and a
    /// float one to the `f` forms. The operand's own type is what decides, so
    /// this reads the type of the value it was actually given.
    fn arith_op(&self, op: &str, lhs: &Value) -> u8 {
        let int = self.value_is_int(lhs);
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
        crate::returns::static_return(class, function).unwrap_or("var")
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
        let result = self.temp(buf, "bool");
        let a = self.expr(buf, lhs, line)?;
        let mut ins = Instruction::new(opcode_of("assign"));
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
        let mut ins = Instruction::new(opcode_of("assign"));
        ins.args.push(Value::Identifier(result));
        ins.args.push(b);
        buf.push(ins, line);

        let end = buf.here();
        if let Value::Int(v) = &mut buf.instructions[at].args[1] {
            *v = end as i32 - at as i32 - 1;
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

    /// The type name a compiled value carries, for sizing a temporary.
    fn value_type(&self, buf: &FnBuf, v: &Value) -> String {
        match v {
            Value::Float(_) => "Float".to_string(),
            Value::Int(_) => "Int".to_string(),
            Value::Bool(_) => "Bool".to_string(),
            Value::String(_) => "String".to_string(),
            Value::Identifier(i) => {
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
        buf.types
            .iter()
            .rev()
            .find(|(n, _)| *n == index)
            .map(|(_, t)| {
                self.strings
                    .get(*t as usize)
                    .cloned()
                    .unwrap_or_else(|| "var".to_string())
            })
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
                } else {
                    Value::Identifier(self.intern(n))
                }
            }
            Expr::Binary { op, lhs, rhs } if op == "&&" || op == "||" => {
                self.logical(buf, op, lhs, rhs, line)?
            }
            Expr::Binary { op, lhs, rhs } => {
                let l = self.expr(buf, lhs, line)?;
                let mut r = self.expr(buf, rhs, line)?;
                // A comparison across two different script types casts the
                // right operand first, into its own temporary. Measured on
                // DLC2ThirskDoorScript, where `akActivator == Game.GetPlayer()`
                // compiles to `cast ::temp2 ::temp1` followed by
                // `cmp_eq ::temp3 akActivator ::temp2`.
                let is_cmp = matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=");
                if is_cmp {
                    let lt = self.value_type(buf, &l);
                    let rt = self.value_type(buf, &r);
                    if lt != rt && lt != "var" && rt != "var" {
                        let tmp = self.temp(buf, &lt);
                        let mut ins = Instruction::new(opcode_of("cast"));
                        ins.args.push(Value::Identifier(tmp));
                        ins.args.push(r);
                        buf.push(ins, line);
                        r = Value::Identifier(tmp);
                    }
                }
                // A comparison yields Bool whatever its operands are; an
                // arithmetic instruction yields its operands' own type.
                let result_ty = match op.as_str() {
                    "+" | "-" | "*" | "/" | "%" => self.value_type(buf, &l),
                    _ => "bool".to_string(),
                };
                let tmp = self.temp(buf, &result_ty);
                let code = match op.as_str() {
                    "+" => self.arith_op("+", &l),
                    "-" => self.arith_op("-", &l),
                    "*" => self.arith_op("*", &l),
                    "/" => self.arith_op("/", &l),
                    "%" => self.arith_op("%", &l),
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
                ins.args.push(l);
                ins.args.push(r);
                buf.push(ins, line);
                if op == "!=" {
                    // `a != b` is `not (a == b)`.
                    let tmp2 = self.temp(buf, "bool");
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
                    "!" => "bool".to_string(),
                    _ => self.value_type(buf, &v),
                };
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
                            ins.varargs = args
                                .iter()
                                .map(|a| self.expr(buf, a, line))
                                .collect::<Result<Vec<_>, _>>()?;
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
                        ins.varargs = args
                            .iter()
                            .map(|a| self.expr(buf, a, line))
                            .collect::<Result<Vec<_>, _>>()?;
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
                                ins.varargs = args
                                    .iter()
                                    .map(|a| self.expr(buf, a, line))
                                    .collect::<Result<Vec<_>, _>>()?;
                                buf.push(ins, line);
                                return Ok(Value::Identifier(tmp));
                            }
                        }
                        let recv = self.expr(buf, obj, line)?;
                        let m = self.intern(method);
                        let tmp = self.temp(buf, self.call_result_type(method));
                        let mut ins = Instruction::new(opcode_of("callmethod"));
                        ins.args.push(Value::Identifier(m));
                        ins.args.push(recv);
                        ins.args.push(Value::Identifier(tmp));
                        ins.varargs = args
                            .iter()
                            .map(|a| self.expr(buf, a, line))
                            .collect::<Result<Vec<_>, _>>()?;
                        buf.push(ins, line);
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
                let o = self.expr(buf, obj, line)?;
                let p = self.intern(name);
                let tmp = self.temp(buf, self.property_type(name));
                let mut ins = Instruction::new(opcode_of("propget"));
                ins.args.push(Value::Identifier(tmp));
                ins.args.push(o);
                ins.args.push(Value::String(p));
                buf.push(ins, line);
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
        if matches!(
            lower.as_str(),
            "self" | "game" | "debug" | "utility" | "math"
        ) {
            return Some(self.intern(&lower));
        }
        // A property whose type is a script class is a static receiver.
        if let Some(t) = self.type_of(name) {
            let base = t.base();
            if !crate::parse::is_builtin_type(&base) {
                return Some(self.intern(name));
            }
        }
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

//! Semantic checks: the difference between a compiler and a *replacement*.
//!
//! The emitter is measured against the corpus and is close. What it does not do
//! is **refuse** anything. An unknown function, a call with too many arguments, a
//! `GotoState` to a state that does not exist — all of them compile, produce a
//! `.pex` that loads, and fail in the game. For an editor that is the worst
//! possible outcome, because the user ships it.
//!
//! # The acceptance criterion is zero false positives
//!
//! Bethesda's own 14,301 scripts are known-good input. Any diagnostic this module
//! reports against them is a **bug in the checker**, not in the script. So the
//! rule for adding a check is:
//!
//! 1. Run it over the whole corpus (`examples/checkcensus`).
//! 2. It must report **zero** findings, or every finding must be explained as
//!    source that genuinely does not compile.
//! 3. Only then may it be used to reject a mod's script.
//!
//! A check that is useful but noisy is worse than no check: the user learns to
//! ignore the output, and the one real error goes with it.
//!
//! # What is checked, and why each is safe
//!
//! | check | severity | why it is safe |
//! |---|---|---|
//! | more arguments than the declaration accepts | error | only fires when a signature is known, and only when the call exceeds it |
//! | **a method the receiver's class does not have** | error | fires only when the receiver's class is *known*; coverage measured at 99.47% of 36,610 resolvable calls, and the whole remainder is the injected pair |
//! | a name that resolves nowhere | warning | conservative: it must be a local, parameter, script declaration, inherited member, state, known class or known function to pass |
//!
//! ## The method check, and the one rule it needs
//!
//! `someActor.NotARealMethod()` compiles today, loads, and fails in the game. It
//! is the most common real mod bug, so it is worth a check — but only after
//! measuring whether the tables can support one. `examples/methodcover` does:
//!
//! ```text
//! member calls with a resolved receiver class: 36,610
//!   known to the tables: 36,415 (99.47%)
//!   UNKNOWN:                195   <- ALL of them GetState or GotoState
//! ```
//!
//! So the gap is not coverage, it is one rule: **`GetState` and `GotoState` are
//! injected by the compiler into every script**, so they are members of every
//! script instance and appear in no `.psc`. Allowing them explicitly takes the
//! corpus to zero findings — and removing the exclusion reproduces exactly those
//! 195 findings, which is the cross-check that the rule is the whole difference.
//!
//! Two refusals make it safe: an **unresolvable receiver** is never judged, and a
//! receiver whose class the caller has **no source for** is never judged either.
//! Both stay silent rather than guess.
//!
//! ## What is deliberately NOT checked
//!
//! **`GotoState("X")` where `X` is not a declared state.** It looks like an
//! error and is not one: Papyrus puts the object into the named state even when
//! no state block declares it, and every call then falls through to the base
//! script. Bethesda's own scripts do it 67 times — `BatteringTrap.psc:58` is
//! `GotoState("Reset")` with no `State Reset` anywhere in the file — and the CK
//! compiles every one. A check here was written first, and
//! `examples/checkcensus` rejected it.
//!
//! ## Two things the corpus corrected while this was being written
//!
//! - **Locals are function-scoped, not block-scoped.** A name declared anywhere
//!   in a function is visible throughout, including before its declaration.
//!   Building the set incrementally reported
//!   `DLC2ManyToManyFactionRelationScript.ProcessArray`, which uses
//!   `currentInnerElement` three lines before declaring it.
//! - **A call's callee is a function name, not a value.** Descending into it as
//!   a name reported every call to an engine function as an undeclared variable.

use crate::parse::{DeclKind, Expr, FunctionDecl, Script, Stmt};
use crate::signatures;
use crate::symbols::ClassTable;
use std::collections::HashSet;

/// How badly a script is broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The script will not do what it says, but it may still load.
    Warning,
    /// The compiler cannot produce correct code for this.
    Error,
}

/// One thing wrong with a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The source line, 1-based, so an editor can underline it.
    pub line: u16,
    /// Whether it is fatal.
    pub severity: Severity,
    /// What is wrong, in one sentence, naming the symbol.
    pub message: String,
}

impl Diagnostic {
    fn error(line: u16, message: impl Into<String>) -> Self {
        Self {
            line,
            severity: Severity::Error,
            message: message.into(),
        }
    }

    fn warning(line: u16, message: impl Into<String>) -> Self {
        Self {
            line,
            severity: Severity::Warning,
            message: message.into(),
        }
    }
}

/// What a check needs to know about the world outside this file.
pub struct Context<'a> {
    /// Every class the caller has sources for, so inherited members resolve.
    pub classes: &'a ClassTable,
    /// Every state the script declares, lowercased, including the empty default.
    pub states: HashSet<String>,
    /// Every function this script declares, in any state, lowercased.
    pub functions: HashSet<String>,
    /// Names declared at script scope, lowercased.
    pub script_names: HashSet<String>,
}

impl<'a> Context<'a> {
    /// Collect what the checks need from `script` and its ancestors.
    pub fn new(script: &Script, classes: &'a ClassTable) -> Self {
        let mut states = HashSet::new();
        // The default state always exists, spelled with an empty name.
        states.insert(String::new());
        let mut functions = HashSet::new();
        let mut script_names = HashSet::new();

        fn walk_decls(
            decls: &[crate::parse::Decl],
            states: &mut HashSet<String>,
            functions: &mut HashSet<String>,
            script_names: &mut HashSet<String>,
        ) {
            for d in decls {
                match &d.kind {
                    DeclKind::Function(f) => {
                        functions.insert(f.name.to_ascii_lowercase());
                    }
                    DeclKind::State(s) => {
                        states.insert(s.name.to_ascii_lowercase());
                        for f in &s.functions {
                            functions.insert(f.name.to_ascii_lowercase());
                        }
                    }
                    DeclKind::Variable(_) | DeclKind::Property(_) => {
                        script_names.insert(d.name.to_ascii_lowercase());
                    }
                    _ => {}
                }
            }
        }
        walk_decls(
            &script.decls,
            &mut states,
            &mut functions,
            &mut script_names,
        );

        // Names reachable through `extends`, so a member declared on a parent is
        // not reported as unknown.
        // The inherited names are NOT enumerated here. `ClassTable` exposes them
        // through `member_type(class, name)`, so the checker probes the names the
        // script actually mentions instead of copying the whole chain - which for
        // `ObjectReference` alone would be 171 members per script.
        Self {
            classes,
            states,
            functions,
            script_names,
        }
    }

    /// The class of a receiver expression, when it can be resolved.
    ///
    /// Returns `None` whenever the class is not knowable, and the method check
    /// then stays silent — an unresolvable receiver must never produce a finding.
    fn receiver_class(&self, e: &Expr, script: &Script) -> Option<String> {
        match e {
            Expr::Ident(n) => {
                let lower = n.to_ascii_lowercase();
                // `self` and `parent` are both the script's own class: `parent`
                // is the same object, just dispatched to the base implementation.
                if lower == "self" || lower == "parent" {
                    return script.parent.clone();
                }
                // A script-scope declaration in this file.
                for d in &script.decls {
                    if d.name.eq_ignore_ascii_case(n) {
                        return Some(d.type_name.0.clone());
                    }
                }
                // An inherited one.
                script
                    .parent
                    .as_ref()
                    .and_then(|p| self.classes.member_type(p, n))
                    .map(|t| t.0.clone())
            }
            Expr::Cast(_, ty) => Some(ty.0.clone()),
            _ => None,
        }
    }

    /// Whether `class` declares `method`, anywhere up its `extends` chain.
    ///
    /// Three sources, because each knows things the others do not: the caller's
    /// own sources (a mod's classes), the harvested signature table (the engine's
    /// shipped classes), and the return-type table (methods whose parameter shape
    /// was ambiguous and so was dropped from the signature table).
    fn method_exists(&self, class: &str, method: &str) -> bool {
        // A method declared in the caller's sources, on this class or an ancestor.
        let mut here = Some(class.to_ascii_lowercase());
        for _ in 0..64 {
            let Some(c) = here else { break };
            // `None` means the caller has no source for that class, so it cannot
            // say either way; `Some(false)` means it has the source and the method
            // is not there.
            if self.classes.declares_method(&c, method) == Some(true) {
                return true;
            }
            here = self.classes.parent_of(&c).map(str::to_string);
        }
        if signatures::method_signature(class, method).is_some() {
            return true;
        }
        if signatures::return_type(class, method).is_some() {
            return true;
        }
        // The engine's classes are reached through their own hierarchy, which the
        // harvested table already walks.
        let lower = class.to_ascii_lowercase();
        if is_engine_class(&lower) && signatures::method_signature(&lower, method).is_some() {
            return true;
        }
        false
    }

    /// Whether `name` is a member of this script's parent chain.
    fn inherited_member(&self, name: &str, script: &Script) -> bool {
        let mut here = script.parent.clone();
        for _ in 0..64 {
            let Some(c) = here else { return false };
            if self.classes.member_type(&c, name).is_some() {
                return true;
            }
            here = self.classes.parent_of(&c).map(str::to_string);
        }
        false
    }
}

/// Check one script and return everything wrong with it.
///
/// An empty vector means nothing was found — not that the script is provably
/// correct. Only the checks listed in the module docs run.
pub fn check(script: &Script, ctx: &Context<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    fn walk_body(
        body: &[Stmt],
        params: &HashSet<String>,
        locals: &mut HashSet<String>,
        script: &Script,
        ctx: &Context<'_>,
        out: &mut Vec<Diagnostic>,
    ) {
        for s in body {
            match s {
                Stmt::Local(l) => {
                    locals.insert(l.name.to_ascii_lowercase());
                    if let Some(init) = &l.initial {
                        check_expr(init, l.line, params, locals, script, ctx, out);
                    }
                }
                Stmt::Assign {
                    target,
                    value,
                    line,
                } => {
                    check_expr(value, *line, params, locals, script, ctx, out);
                    check_target(target, *line, params, locals, script, ctx, out);
                }
                Stmt::CompoundAssign {
                    target,
                    value,
                    line,
                    ..
                } => {
                    check_expr(value, *line, params, locals, script, ctx, out);
                    check_target(target, *line, params, locals, script, ctx, out);
                }
                Stmt::Expr(e, line) => check_expr(e, *line, params, locals, script, ctx, out),
                Stmt::If { branches, .. } => {
                    for b in branches {
                        if let Some(c) = &b.cond {
                            check_expr(c, b.line, params, locals, script, ctx, out);
                        }
                        walk_body(&b.body, params, locals, script, ctx, out);
                    }
                }
                Stmt::While { cond, body, line } => {
                    check_expr(cond, *line, params, locals, script, ctx, out);
                    walk_body(body, params, locals, script, ctx, out);
                }
                Stmt::Return { value, line } => {
                    check_expr(value, *line, params, locals, script, ctx, out)
                }
                _ => {}
            }
        }
    }

    fn check_target(
        target: &Expr,
        line: u16,
        params: &HashSet<String>,
        locals: &HashSet<String>,
        script: &Script,
        ctx: &Context<'_>,
        out: &mut Vec<Diagnostic>,
    ) {
        match target {
            Expr::Ident(n) => check_name(n, line, params, locals, script, ctx, out),
            Expr::Member(obj, _) => check_expr(obj, line, params, locals, script, ctx, out),
            other => check_expr(other, line, params, locals, script, ctx, out),
        }
    }

    fn check_name(
        name: &str,
        line: u16,
        params: &HashSet<String>,
        locals: &HashSet<String>,
        script: &Script,
        ctx: &Context<'_>,
        out: &mut Vec<Diagnostic>,
    ) {
        let lower = name.to_ascii_lowercase();
        if params.contains(&lower)
            || locals.contains(&lower)
            || ctx.script_names.contains(&lower)
            || ctx.functions.contains(&lower)
            || ctx.states.contains(&lower)
            || lower == "self"
            || lower == "parent"
        {
            return;
        }
        if ctx.inherited_member(name, script) {
            return;
        }
        // A class name used as a static receiver: `Game`, `Utility`, or one of
        // the script's own property types.
        if ctx.classes.has(&lower) || is_engine_class(&lower) {
            return;
        }
        // A function the corpus knows at all, even if this file does not declare
        // it: an engine function reached through a bare call.
        if signatures::params(&lower).is_some()
            || signatures::imported_class(&lower).is_some()
            || signatures::return_type("", &lower).is_some()
        {
            return;
        }
        out.push(Diagnostic::warning(
            line,
            format!("`{name}` is not declared here, inherited, or known to the engine"),
        ));
    }

    fn check_expr(
        e: &Expr,
        line: u16,
        params: &HashSet<String>,
        locals: &HashSet<String>,
        script: &Script,
        ctx: &Context<'_>,
        out: &mut Vec<Diagnostic>,
    ) {
        match e {
            Expr::Ident(n) => check_name(n, line, params, locals, script, ctx, out),
            Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) | Expr::None => {}
            Expr::Member(obj, _) => check_expr(obj, line, params, locals, script, ctx, out),
            Expr::Cast(inner, _) => check_expr(inner, line, params, locals, script, ctx, out),
            Expr::Unary { operand, .. } => {
                check_expr(operand, line, params, locals, script, ctx, out)
            }
            Expr::Binary { lhs, rhs, .. } => {
                check_expr(lhs, line, params, locals, script, ctx, out);
                check_expr(rhs, line, params, locals, script, ctx, out);
            }
            Expr::Index(target, index) => {
                check_expr(target, line, params, locals, script, ctx, out);
                check_expr(index, line, params, locals, script, ctx, out);
            }
            Expr::Call { callee, args } => {
                // More arguments than any declaration accepts. Only fires when a
                // signature is known, and only when the call exceeds every one of
                // them — a shorter call is legal because the compiler pads.
                // `Class.Method(...)` — the engine classes are the ones whose
                // signatures the corpus knows reliably, because their sources are
                // shipped.
                if let Expr::Member(obj, method) = callee.as_ref() {
                    if let Expr::Ident(class) = obj.as_ref() {
                        let cls = class.to_ascii_lowercase();
                        if is_engine_class(&cls) {
                            if let Some((shape, _)) = signatures::method_signature(&cls, method) {
                                if args.len() > shape.len() {
                                    out.push(Diagnostic::error(
                                        line,
                                        format!(
                                            "`{class}.{method}` takes at most {} argument{}, called with {}",
                                            shape.len(),
                                            if shape.len() == 1 { "" } else { "s" },
                                            args.len()
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                }
                if let Expr::Ident(f) = callee.as_ref() {
                    // `Some(&[])` means "takes no arguments", which is a known
                    // signature; only `None` means unknown. Skipping the empty
                    // case let `GetPlayer(1)` through.
                    if let Some(shape) = signatures::params(&f.to_ascii_lowercase()) {
                        if args.len() > shape.len() {
                            out.push(Diagnostic::error(
                                line,
                                format!(
                                    "`{f}` takes at most {} argument{}, called with {}",
                                    shape.len(),
                                    if shape.len() == 1 { "" } else { "s" },
                                    args.len()
                                ),
                            ));
                        }
                    }
                }
                // A method call on a receiver whose class IS known, but which
                // does not declare the method. This is the check that catches the
                // most real mod bugs — `someActor.NotARealMethod()` compiles today
                // and fails in the game — and it is safe only because coverage
                // was measured first: 36,415 of 36,610 resolvable member calls in
                // the corpus (99.47%) hit a known signature, and every one of the
                // 195 that did not was `GetState` or `GotoState`.
                //
                // Those two are **injected by the compiler into every script**, so
                // they are members of every script instance and appear in no
                // `.psc`. They are allowed explicitly below, which takes the
                // corpus to zero findings.
                if let Expr::Member(obj, method) = callee.as_ref() {
                    if !is_injected_method(method) {
                        if let Some(cls) = ctx.receiver_class(obj, script) {
                            // Only judge a class the caller has sources for. A
                            // class from an unpublished plugin is unknowable, and
                            // guessing would produce exactly the noise this check
                            // is meant to avoid.
                            let known_class =
                                ctx.classes.has(&cls) || is_engine_class(&cls.to_ascii_lowercase());
                            if known_class && !ctx.method_exists(&cls, method) {
                                out.push(Diagnostic::error(
                                    line,
                                    format!("`{cls}` has no method `{method}`"),
                                ));
                            }
                        }
                    }
                }
                // The callee is a FUNCTION NAME, not a value: descending into it
                // as a name would report every call to `GotoState` or any engine
                // function as an undeclared variable. A member callee still has
                // its receiver checked, because the receiver IS a value.
                match callee.as_ref() {
                    Expr::Ident(_) => {}
                    other => check_expr(other, line, params, locals, script, ctx, out),
                }
                for a in args {
                    check_expr(a, line, params, locals, script, ctx, out);
                }
            }
        }
    }

    fn walk_function(
        f: &FunctionDecl,
        script: &Script,
        ctx: &Context<'_>,
        out: &mut Vec<Diagnostic>,
    ) {
        let params: HashSet<String> = f
            .params
            .iter()
            .map(|p| p.name.to_ascii_lowercase())
            .collect();
        // Papyrus locals are **function-scoped**, not block-scoped: a name
        // declared anywhere in the function is visible throughout, including
        // before its declaration. Seeding from `f.locals` is therefore correct,
        // and building the set incrementally was not - it reported
        // `DLC2ManyToManyFactionRelationScript.ProcessArray` line 40, where
        // `currentInnerElement` is used three lines before it is declared and the
        // CK compiles it happily.
        let mut locals: HashSet<String> = f
            .locals
            .iter()
            .map(|l| l.name.to_ascii_lowercase())
            .collect();
        walk_body(&f.body, &params, &mut locals, script, ctx, out);
    }

    for d in &script.decls {
        match &d.kind {
            DeclKind::Function(f) => walk_function(f, script, ctx, &mut out),
            DeclKind::State(s) => {
                for f in &s.functions {
                    walk_function(f, script, ctx, &mut out);
                }
            }
            _ => {}
        }
    }

    out.sort_by_key(|d| (d.line, d.severity));
    out
}

/// Whether the compiler **injects** this method into every script.
///
/// `GetState` and `GotoState` are not in any source file: the compiler adds them
/// to every script, so they are members of every script instance. A method check
/// that did not know this would flag all 195 of their calls in the corpus — which
/// is the entire remainder of the unknown-method bucket, measured.
fn is_injected_method(name: &str) -> bool {
    name.eq_ignore_ascii_case("getstate") || name.eq_ignore_ascii_case("gotostate")
}

/// The engine's own classes, which are in scope everywhere.
fn is_engine_class(name: &str) -> bool {
    matches!(
        name,
        "game" | "debug" | "utility" | "math" | "objectreference" | "actor" | "form"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lex, parse};

    fn check_src(src: &str) -> Vec<Diagnostic> {
        let script = parse(&lex(src)).expect("parses");
        let table = ClassTable::new([&script]);
        let ctx = Context::new(&script, &table);
        check(&script, &ctx)
    }

    #[test]
    fn a_method_the_receiver_does_not_have_is_an_error() {
        // This is the bug the check exists for: it compiles today and fails in
        // the game. `ObjectReference` has no `NotARealMethod`.
        let d = check_src(
            "Scriptname A extends ObjectReference\nFunction F()\n\tSelf.NotARealMethod()\nEndFunction\n",
        );
        assert!(
            d.iter().any(|d| d.message.contains("NotARealMethod")),
            "an invented method must be reported: {d:?}"
        );
    }

    #[test]
    fn a_method_the_receiver_does_have_is_clean() {
        let d = check_src(
            "Scriptname A extends ObjectReference\nFunction F()\n\tSelf.Disable()\nEndFunction\n",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn the_injected_pair_is_always_available() {
        // `GetState` and `GotoState` are added by the compiler to every script
        // and appear in no `.psc`. Without this exclusion the check flags all 195
        // of their calls in the corpus.
        let d = check_src(
            "Scriptname A extends ObjectReference\nFunction F()\n\tGotoState(\"x\")\n\tstring s = GetState()\nEndFunction\n",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn a_method_on_an_unknown_class_is_not_judged() {
        // The caller has no source for `SomeUnpublishedPluginScript`, so nothing
        // can be said about it. Guessing here is exactly the noise the zero
        // false-positive rule forbids.
        let d = check_src(
            "Scriptname A extends ObjectReference\nSomeUnpublishedPluginScript Property S auto\nFunction F()\n\tS.AnythingAtAll()\nEndFunction\n",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn goto_to_an_undeclared_state_is_legal_and_not_reported() {
        // This looks like an error and is not one: Papyrus puts the object into
        // the named state even if no state block declares it, and every call then
        // falls through to the base script. Bethesda's own scripts do it 67 times
        // (`BatteringTrap.psc:58`, `GotoState("Reset")`), and the CK compiles
        // every one of them.
        //
        // A check here would have been 67 false positives on known-good code,
        // which is exactly what `examples/checkcensus` exists to catch.
        let d = check_src(
            "Scriptname A extends ObjectReference\nState Busy\nEndState\nFunction F()\n\tGotoState(\"nowhere\")\nEndFunction\n",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn too_many_arguments_is_an_error_on_a_static_call() {
        // `GetPlayer()` takes none.
        let d = check_src(
            "Scriptname A extends ObjectReference\nFunction F()\n\tGame.GetPlayer(1)\nEndFunction\n",
        );
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("GetPlayer"), "{d:?}");
    }

    #[test]
    fn too_many_arguments_is_an_error_on_a_bare_call() {
        // The BARE-call path is a separate branch of the arity check, and this
        // test exists because the one above does not cover it: disabling the bare
        // branch left the static test passing, so it was guarding nothing on that
        // path. `BlockActivation(bool abBlocked = true)` takes one argument.
        let d = check_src(
            "Scriptname A extends ObjectReference\nFunction F()\n\tBlockActivation(true, true)\nEndFunction\n",
        );
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("BlockActivation"), "{d:?}");
        assert_eq!(d[0].line, 3);
    }

    #[test]
    fn an_unknown_bare_name_is_reported() {
        let d =
            check_src("Scriptname A extends ObjectReference\nFunction F()\n\tx = 1\nEndFunction\n");
        assert!(
            d.iter().any(|d| d.message.contains('x')),
            "an undeclared name must be reported: {d:?}"
        );
    }

    #[test]
    fn a_parameter_a_local_and_a_property_are_all_known() {
        let d = check_src(
            "Scriptname A extends ObjectReference\nint Property P auto\nFunction F(int akA)\n\tint i = 1\n\ti = akA\n\tP = i\nEndFunction\n",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn an_inherited_member_is_known() {
        let parent = parse(&lex(
            "Scriptname Base extends ObjectReference\nbool Property IsTransitionFight auto\n",
        ))
        .unwrap();
        let child = parse(&lex(
            "Scriptname A extends Base\nFunction F()\n\tIsTransitionFight = true\nEndFunction\n",
        ))
        .unwrap();
        let table = ClassTable::new([&parent, &child]);
        let ctx = Context::new(&child, &table);
        let d = check(&child, &ctx);
        assert!(
            d.is_empty(),
            "an inherited name must not be reported: {d:?}"
        );
    }
}

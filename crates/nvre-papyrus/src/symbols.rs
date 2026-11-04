//! Building the caller-supplied symbol table for an editor.
//!
//! [`Compiler`](crate::emit::Compiler) resolves a bare name in three places: the
//! function's own scope, the script's own declarations, and — last — the
//! [`Symbols`] the caller passes in. The third is what a single-file compile
//! cannot supply, and what an editor needs.
//!
//! # Why a single file is not enough
//!
//! A Papyrus script sees every member it **inherits**:
//!
//! ```text
//! Scriptname ArenaTransitionCombatQuest extends ArenaCombatQuest
//!     IsTransitionFight = isTransition
//! ```
//!
//! `IsTransitionFight` is declared in `ArenaCombatQuest.psc`, which is a
//! different file. Compiling `ArenaTransitionCombatQuest.psc` alone, the name is
//! not in the script's declaration list and not in the function's scope, so it
//! types as nothing: no conversion fires at the slot it is written to, and an
//! arithmetic expression involving it picks its opcode from `var`.
//!
//! The shipped corpus hides this, because the *engine's* classes are in the
//! harvested tables in [`crate::signatures`]. A mod's classes are not — nothing
//! harvested them — so a mod script is exactly the case that breaks.
//!
//! # What this does
//!
//! Given every `.psc` the user has, build the symbol table for one script: its
//! own variables and properties, plus those of every ancestor reachable through
//! `extends`. Nearest declaration wins, so an override shadows the class it
//! overrides.
//!
//! ```text
//! let mod_scripts = parse_all(&sources);          // every .psc in the mod
//! let symbols = Symbols::for_script(&script, &mod_scripts);
//! let compiled = Compiler::new(&script, symbols).compile()?;
//! ```

use crate::emit::Symbols;
use crate::parse::{DeclKind, Script, TypeName};
use std::collections::HashMap;

/// Every declaration in a set of parsed scripts, by lowercased class name.
///
/// Built once per mod and reused for every script compiled against it, because
/// rebuilding it per script would make compiling N scripts O(N²).
#[derive(Debug, Default, Clone)]
pub struct ClassTable {
    /// Lowercased class name -> its variables and properties, and its parent.
    classes: HashMap<String, ClassInfo>,
}

/// What one class declares.
#[derive(Debug, Default, Clone)]
struct ClassInfo {
    parent: Option<String>,
    /// Variables and properties, in declaration order, keyed by lowercased name.
    members: HashMap<String, TypeName>,
}

impl ClassTable {
    /// Build the table from every script the caller has.
    pub fn new<'a>(scripts: impl IntoIterator<Item = &'a Script>) -> Self {
        let mut classes = HashMap::new();
        for s in scripts {
            let mut members = HashMap::new();
            for d in &s.decls {
                match &d.kind {
                    // A script-scope variable is visible to subclasses.
                    DeclKind::Variable(_) | DeclKind::Property(_) => {
                        members
                            .entry(d.name.to_ascii_lowercase())
                            .or_insert_with(|| d.type_name.clone());
                    }
                    _ => {}
                }
            }
            classes.insert(
                s.name.to_ascii_lowercase(),
                ClassInfo {
                    parent: s.parent.as_ref().map(|p| p.to_ascii_lowercase()),
                    members,
                },
            );
        }
        Self { classes }
    }

    /// Whether the table knows this class.
    pub fn has(&self, class: &str) -> bool {
        self.classes.contains_key(&class.to_ascii_lowercase())
    }

    /// Every class in the table, lowercased.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.classes.keys().map(String::as_str)
    }

    /// The declared type of `member` on `class` or any ancestor.
    ///
    /// Walks the chain outward, so the nearest declaration wins.
    pub fn member_type(&self, class: &str, member: &str) -> Option<&TypeName> {
        let member = member.to_ascii_lowercase();
        let mut here = Some(class.to_ascii_lowercase());
        // A cycle in the `extends` chain is a source error, not a crash.
        for _ in 0..64 {
            let c = here.take()?;
            let info = self.classes.get(&c)?;
            if let Some(t) = info.members.get(&member) {
                return Some(t);
            }
            here = info.parent.clone();
        }
        None
    }

    /// The parent of `class`, if the table knows it.
    pub fn parent_of(&self, class: &str) -> Option<&str> {
        self.classes
            .get(&class.to_ascii_lowercase())?
            .parent
            .as_deref()
    }
}

impl Symbols {
    /// The symbol table for compiling `script` against everything it inherits.
    ///
    /// `all` should contain every `.psc` the user has — at minimum the script
    /// itself and each of its ancestors. Anything missing degrades to the old
    /// behaviour (a name that types as nothing), never to a wrong type.
    pub fn for_script(script: &Script, all: &ClassTable) -> Symbols {
        let mut variables = Vec::new();
        let mut properties = Vec::new();
        let mut seen: HashMap<String, ()> = HashMap::new();

        // The script's own declarations first, so they shadow inherited ones.
        for d in &script.decls {
            let name = d.name.to_ascii_lowercase();
            if seen.insert(name.clone(), ()).is_some() {
                continue;
            }
            match &d.kind {
                DeclKind::Variable(_) => variables.push((name, d.type_name.clone())),
                DeclKind::Property(_) => properties.push((name, d.type_name.clone())),
                _ => {
                    // Not a member; do not occupy the name.
                    seen.remove(&name);
                }
            }
        }

        // Then each ancestor, outward. Nearest wins, so an inner declaration is
        // never overwritten by the class it overrides.
        let mut here = script.parent.as_ref().map(|p| p.to_ascii_lowercase());
        for _ in 0..64 {
            let Some(c) = here.take() else { break };
            let Some(info) = all.classes.get(&c) else {
                break;
            };
            for (name, ty) in &info.members {
                if seen.insert(name.clone(), ()).is_some() {
                    continue;
                }
                variables.push((name.clone(), ty.clone()));
            }
            here = info.parent.clone();
        }

        Symbols {
            script: script.name.clone(),
            parent: script.parent.clone().unwrap_or_default(),
            variables,
            properties,
            states: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lex, parse};

    fn parse_one(src: &str) -> Script {
        parse(&lex(src)).expect("test source parses")
    }

    #[test]
    fn an_inherited_member_resolves_to_its_declared_type() {
        let parent = parse_one(
            "Scriptname Base extends ObjectReference\nbool Property IsTransitionFight auto\n",
        );
        let child = parse_one(
            "Scriptname Derived extends Base\nFunction F()\n\tIsTransitionFight = true\nEndFunction\n",
        );
        let table = ClassTable::new([&parent, &child]);
        let syms = Symbols::for_script(&child, &table);

        // This is the whole point: the name is not in `child` at all.
        assert!(child
            .decls
            .iter()
            .all(|d| !d.name.eq_ignore_ascii_case("IsTransitionFight")));
        let found = syms
            .variables
            .iter()
            .find(|(n, _)| n == "istransitionfight")
            .expect("inherited member must be in the table");
        assert_eq!(found.1 .0.to_ascii_lowercase(), "bool");
    }

    #[test]
    fn a_child_declaration_shadows_the_parent() {
        let parent =
            parse_one("Scriptname Base extends ObjectReference\nint Property Value auto\n");
        let child = parse_one("Scriptname Derived extends Base\nfloat Property Value auto\n");
        let table = ClassTable::new([&parent, &child]);
        let syms = Symbols::for_script(&child, &table);

        // `Value` is a Property, so it lands in `properties`; `type_of` consults
        // both lists, and the assertion must too.
        let hits: Vec<_> = syms
            .variables
            .iter()
            .chain(&syms.properties)
            .filter(|(n, _)| n == "value")
            .collect();
        assert_eq!(hits.len(), 1, "a shadowed member is listed once");
        assert_eq!(hits[0].1 .0.to_ascii_lowercase(), "float");
    }

    #[test]
    fn a_grandparent_is_reached() {
        let a = parse_one("Scriptname A extends ObjectReference\nint Property Deep auto\n");
        let b = parse_one("Scriptname B extends A\n");
        let c = parse_one("Scriptname C extends B\n");
        let table = ClassTable::new([&a, &b, &c]);
        let syms = Symbols::for_script(&c, &table);
        assert!(
            syms.variables.iter().any(|(n, _)| n == "deep"),
            "the chain must be walked past the direct parent"
        );
    }

    #[test]
    fn an_unknown_parent_is_not_an_error() {
        // The parent's source may simply not be available. That degrades to "no
        // type", which is the old behaviour - never to a wrong type.
        let s = parse_one("Scriptname Orphan extends NowhereToBeFound\n");
        let table = ClassTable::new([&s]);
        let syms = Symbols::for_script(&s, &table);
        assert!(syms.variables.is_empty());
    }

    #[test]
    fn a_cycle_in_the_chain_terminates() {
        // `A extends B` and `B extends A` is a source error; it must not hang.
        let a = parse_one("Scriptname A extends B\nint Property X auto\n");
        let b = parse_one("Scriptname B extends A\nint Property Y auto\n");
        let table = ClassTable::new([&a, &b]);
        let syms = Symbols::for_script(&a, &table);
        let has = |want: &str| {
            syms.variables
                .iter()
                .chain(&syms.properties)
                .any(|(n, _)| n == want)
        };
        assert!(has("x"));
        assert!(has("y"));
    }
}

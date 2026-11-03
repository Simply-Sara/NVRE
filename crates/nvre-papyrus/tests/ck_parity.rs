//! Regressions against the real compiler's own output.
//!
//! Each fixture is a pair: a `.psc` this crate compiles, and the `.pex` the
//! **Creation Kit compiler** produced from that same source. The test asserts
//! instruction-for-instruction equality, so a change that quietly alters codegen
//! fails here instead of being noticed a corpus run later.
//!
//! Why this exists: a `propset` bug — the value not being materialised into a
//! temporary — sat in the tree for a full round and cost 825 functions. One test
//! over these fixtures would have caught it the moment it was introduced.
//!
//! The fixtures cover the constructs where the emitter has been wrong before:
//! if-chains, nested ifs, `elseif`, casts at every typed slot, `None` as an
//! argument, and static-call default padding. They are deliberately small — one
//! construct per file — so a failure names the construct that broke.
//!
//! Adding a fixture: compile a `.psc` with the real compiler, drop both files in
//! `tests/probes/`, and the test picks it up automatically. Do **not** add one
//! for a construct the emitter already gets wrong; a suite that starts red is
//! not a suite.

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};
use std::path::{Path, PathBuf};

/// One instruction rendered as comparable text.
///
/// Every operand is resolved through the string table it belongs to, and
/// `varargs` is included — an instruction's arguments live there, not in
/// `args`, and a comparison that skips them sees two different calls as equal.
fn instr_text(
    strings: &[String],
    get: impl Fn(u16) -> String,
    i: &nvre_papyrus::pex::Instruction,
) -> String {
    let mut out = opcodes::name(i.opcode).to_string();
    for v in i.args.iter().chain(i.varargs.iter()) {
        out.push(' ');
        out.push_str(&match v {
            Value::None => "None".to_string(),
            Value::Identifier(x) => get(*x),
            Value::String(x) => format!("{:?}", get(*x)),
            Value::Int(x) => x.to_string(),
            Value::Float(x) => format!("{x}"),
            Value::Bool(x) => x.to_string(),
        });
    }
    let _ = strings;
    out
}

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/probes");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/probes must exist")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "psc").unwrap_or(false))
        .collect();
    v.sort();
    v
}

#[test]
fn the_emitter_reproduces_the_real_compilers_output() {
    let mut checked = 0;
    let mut failures = Vec::new();

    for src_path in fixtures() {
        let stem = src_path.file_stem().unwrap().to_string_lossy().to_string();
        let pex_path = src_path.with_extension("pex");

        let source = nvre_util::cp1252::decode(&std::fs::read(&src_path).unwrap());
        let script = parse(&lex(&source)).unwrap_or_else(|e| panic!("{stem}: parse failed: {e:?}"));
        let symbols = Symbols {
            script: script.name.clone(),
            parent: script.parent.clone().unwrap_or_default(),
            variables: Vec::new(),
            properties: Vec::new(),
            states: Vec::new(),
        };
        let compiled = Compiler::new(&script, symbols)
            .compile()
            .unwrap_or_else(|e| panic!("{stem}: emit failed: {e:?}"));

        let game = PexFile::parse(&std::fs::read(&pex_path).unwrap())
            .unwrap_or_else(|e| panic!("{stem}: fixture .pex is unreadable: {e:?}"));

        // Compare function by function, by NAME: the two compilers lay the
        // record order out differently, and position is not the thing under
        // test here.
        let game_funcs: Vec<_> = game
            .objects
            .iter()
            .flat_map(|o| o.states.iter())
            .flat_map(|s| s.functions.iter())
            .collect();

        for gf in game_funcs {
            let gname = game.text_or_empty(gf.name).to_string();
            let mine = compiled
                .states
                .iter()
                .flat_map(|(_, fs)| fs.iter())
                .find(|f| {
                    compiled
                        .strings
                        .get(f.name as usize)
                        .map(|s| s == &gname)
                        .unwrap_or(false)
                })
                .unwrap_or_else(|| panic!("{stem}: no function `{gname}` in our output"));

            let theirs: Vec<String> = gf
                .instructions
                .iter()
                .map(|i| instr_text(&game.strings, |x| game.text_or_empty(x).to_string(), i))
                .collect();
            let ours: Vec<String> = mine
                .instructions
                .iter()
                .map(|i| {
                    instr_text(
                        &compiled.strings,
                        |x| {
                            compiled
                                .strings
                                .get(x as usize)
                                .cloned()
                                .unwrap_or_default()
                        },
                        i,
                    )
                })
                .collect();

            if theirs != ours {
                let at = theirs
                    .iter()
                    .zip(&ours)
                    .position(|(a, b)| a != b)
                    .unwrap_or(theirs.len().min(ours.len()));
                failures.push(format!(
                    "{stem}::{gname} first differs at {at}\n  game {}\n  mine {}",
                    theirs.get(at).map(String::as_str).unwrap_or("<absent>"),
                    ours.get(at).map(String::as_str).unwrap_or("<absent>"),
                ));
            }
            checked += 1;
        }
    }

    assert!(checked > 0, "no fixtures found in tests/probes");
    assert!(
        failures.is_empty(),
        "{} of {checked} functions differ from the real compiler:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_fixture_has_a_shipped_pex_beside_it() {
    for src in fixtures() {
        assert!(
            src.with_extension("pex").exists(),
            "{} has no .pex fixture — the pair is what makes this a regression test",
            src.display()
        );
    }
}

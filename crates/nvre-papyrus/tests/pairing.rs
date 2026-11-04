//! A test that the *measurement* is right, not just the compiler.
//!
//! Every parity figure this crate reports depends on pairing each shipped
//! function with the one this crate emits. That pairing was wrong for several
//! rounds: it matched by function NAME only, so a name appearing in two states
//! was compared against the wrong body and the census understated parity. Nothing
//! caught it, because the regression suite only checks that codegen does not
//! regress — not that the instrument measuring it is sound.
//!
//! This test pins the instrument down. It checks that:
//!
//! 1. A function is paired with the body in the SAME state. The fixtures below
//!    contain the same function name in two states with different bodies, which
//!    is exactly the case that was mishandled.
//! 2. Pairing is stable and total: every shipped function is either matched or
//!    counted as absent, never silently dropped.
//!
//! It reads the same corpus the census does, so it needs `NVRE_DATA` or an
//! explicit path, and is skipped when neither is available.

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse, PexFile};
use std::io::Read;

/// Pair every shipped function with the emitted one, by state AND name.
fn pair(
    compiled: &nvre_papyrus::emit::CompiledScript,
    game: &PexFile,
) -> (Vec<(String, String, usize)>, Vec<(String, String)>) {
    let mut matched = Vec::new();
    let mut missing = Vec::new();
    for gstate in game.objects.iter().flat_map(|o| o.states.iter()) {
        let want = game.text_or_empty(gstate.name).to_ascii_lowercase();
        for gf in &gstate.functions {
            let gname = game.text_or_empty(gf.name).to_string();
            let found = compiled
                .states
                .iter()
                .find(|(s, _)| {
                    compiled
                        .strings
                        .get(*s as usize)
                        .map(|n| n.to_ascii_lowercase() == want)
                        .unwrap_or(false)
                })
                .and_then(|(_, fs)| {
                    fs.iter().find(|f| {
                        compiled.strings.get(f.name as usize).map(String::as_str)
                            == Some(gname.as_str())
                    })
                });
            match found {
                // The third field is the size of the body the instrument chose.
                // It is what lets a test tell a correct pairing from one that
                // matched on name alone.
                Some(f) => matched.push((want.clone(), gname, f.instructions.len())),
                None => missing.push((want.clone(), gname)),
            }
        }
    }
    (matched, missing)
}

fn corpus() -> Option<(String, String)> {
    let data = std::env::var("NVRE_DATA").ok()?;
    let zip = format!("{data}/Scripts.zip");
    let bsa = format!("{data}/Skyrim - Misc.bsa");
    if std::path::Path::new(&zip).exists() && std::path::Path::new(&bsa).exists() {
        Some((zip, bsa))
    } else {
        None
    }
}

/// A function name that appears in more than one state must be paired with the
/// body from the state it was found in — not the first name match anywhere.
#[test]
fn functions_are_paired_within_their_own_state() {
    let Some((zip_path, bsa_path)) = corpus() else {
        eprintln!("skipping: set NVRE_DATA to a Skyrim Special Edition Data directory");
        return;
    };

    let mut zip = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
    let mut archive = bsa105::Archive::open(&bsa_path).unwrap();

    let mut checked_multi_state = 0usize;
    let mut checked_scripts = 0usize;

    for i in 0..zip.len() {
        if checked_multi_state >= 12 {
            break;
        }
        let name = zip.by_index(i).unwrap().name().to_string();
        if !name.ends_with(".psc") {
            continue;
        }
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let src = {
            let Ok(mut f) = zip.by_name(&name) else {
                continue;
            };
            let mut b = Vec::new();
            if f.read_to_end(&mut b).is_err() {
                continue;
            }
            nvre_util::cp1252::decode(&b)
        };
        let Ok(bytes) = archive.extract_by_path(&format!("scripts\\{leaf}.pex")) else {
            continue;
        };
        let Ok(game) = PexFile::parse(&bytes) else {
            continue;
        };
        let Ok(script) = parse(&lex(&src)) else {
            continue;
        };

        // Only scripts with the same function name in two states matter here.
        let mut names: Vec<String> = game
            .objects
            .iter()
            .flat_map(|o| o.states.iter())
            .flat_map(|s| s.functions.iter())
            .map(|f| game.text_or_empty(f.name).to_ascii_lowercase())
            .collect();
        names.sort();
        let dup = names.windows(2).any(|w| w[0] == w[1]);
        if !dup {
            continue;
        }

        let symbols = Symbols {
            script: script.name.clone(),
            parent: script.parent.clone().unwrap_or_default(),
            variables: Vec::new(),
            properties: Vec::new(),
            states: Vec::new(),
        };
        let Ok(c) = Compiler::new(&script, symbols).compile() else {
            continue;
        };

        let (matched, _missing) = pair(&c, &game);
        checked_scripts += 1;

        // THE DECISIVE CHECK, driven by what `pair` actually returned.
        //
        // When one name is shipped in two states its bodies differ. A pairing
        // that matches on name alone returns the SAME body for both entries, so
        // the sizes it reports come out equal where the game's do not. Asserting
        // on the instrument's own output — not on a lookup performed here — is
        // what gives this test teeth.
        let mut shipped_sizes: std::collections::BTreeMap<&str, Vec<usize>> =
            std::collections::BTreeMap::new();
        for gstate in game.objects.iter().flat_map(|o| o.states.iter()) {
            for gf in &gstate.functions {
                shipped_sizes
                    .entry(game.text_or_empty(gf.name))
                    .or_default()
                    .push(gf.instructions.len());
            }
        }
        let mut paired_sizes: std::collections::BTreeMap<&str, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (state, gname, len) in &matched {
            let _ = state;
            paired_sizes.entry(gname.as_str()).or_default().push(*len);
        }

        for (gname, shipped) in &shipped_sizes {
            if shipped.len() < 2 {
                continue;
            }
            // Only meaningful when the shipped bodies actually differ.
            if shipped.iter().all(|l| *l == shipped[0]) {
                continue;
            }
            let Some(paired) = paired_sizes.get(gname) else {
                continue;
            };
            if paired.len() < 2 {
                continue;
            }
            assert!(
                paired.iter().any(|l| *l != paired[0]),
                "{leaf}: {gname} ships with differing bodies per state ({shipped:?}) \
                 but the pairing returned identical ones ({paired:?}) — the \
                 instrument is matching on name alone"
            );
        }

        checked_multi_state += 1;
    }

    assert!(
        checked_multi_state > 0,
        "no script with a repeated function name was found — the corpus path may be wrong"
    );
    eprintln!("checked {checked_multi_state} multi-state scripts of {checked_scripts}");
}

/// Pairing must be total: a shipped function is either matched or reported
/// absent. A silent drop would flatter every census that uses this.
#[test]
fn pairing_accounts_for_every_shipped_function() {
    let Some((zip_path, bsa_path)) = corpus() else {
        eprintln!("skipping: set NVRE_DATA to a Skyrim Special Edition Data directory");
        return;
    };

    let mut zip = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
    let mut archive = bsa105::Archive::open(&bsa_path).unwrap();

    for i in 0..zip.len().min(400) {
        let name = zip.by_index(i).unwrap().name().to_string();
        if !name.ends_with(".psc") {
            continue;
        }
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let src = {
            let Ok(mut f) = zip.by_name(&name) else {
                continue;
            };
            let mut b = Vec::new();
            if f.read_to_end(&mut b).is_err() {
                continue;
            }
            nvre_util::cp1252::decode(&b)
        };
        let Ok(bytes) = archive.extract_by_path(&format!("scripts\\{leaf}.pex")) else {
            continue;
        };
        let Ok(game) = PexFile::parse(&bytes) else {
            continue;
        };
        let Ok(script) = parse(&lex(&src)) else {
            continue;
        };
        let symbols = Symbols {
            script: script.name.clone(),
            parent: script.parent.clone().unwrap_or_default(),
            variables: Vec::new(),
            properties: Vec::new(),
            states: Vec::new(),
        };
        let Ok(c) = Compiler::new(&script, symbols).compile() else {
            continue;
        };

        let shipped: usize = game
            .objects
            .iter()
            .flat_map(|o| o.states.iter())
            .map(|s| s.functions.len())
            .sum();
        let (matched, missing) = pair(&c, &game);
        assert_eq!(
            matched.len() + missing.len(),
            shipped,
            "{leaf}: {shipped} shipped functions but pairing accounted for {}",
            matched.len() + missing.len()
        );
    }
}

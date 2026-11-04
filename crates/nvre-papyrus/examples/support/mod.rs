//! Shared helper for the corpus probes.
//!
//! A file under `examples/support/` is not compiled as an example of its own,
//! which is why the helper lives in a subdirectory rather than beside the
//! probes.

use nvre_papyrus::parse::Script;
use nvre_papyrus::symbols::ClassTable;
use nvre_papyrus::{lex, parse};
use std::io::Read;

/// Decode one `.psc` from the CK's own archive.
pub fn source_of(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<String> {
    let mut f = zip.by_name(name).ok()?;
    let mut b = Vec::new();
    f.read_to_end(&mut b).ok()?;
    Some(nvre_util::cp1252::decode(&b))
}

/// Every source in the archive, parsed — the input to a [`ClassTable`].
pub fn parse_all(zip: &mut zip::ZipArchive<std::fs::File>, sources: &[String]) -> Vec<Script> {
    let mut out = Vec::with_capacity(sources.len());
    for name in sources {
        if let Some(src) = source_of(zip, name) {
            if let Ok(s) = parse(&lex(&src)) {
                out.push(s);
            }
        }
    }
    out
}

/// The class table an editor would have: every script in the corpus, so an
/// inherited name has a type.
///
/// Without it a probe compiles one file in isolation, its parent's declarations
/// are invisible, no conversion fires at a slot typed by them, and the call comes
/// out an instruction short. Measured on
/// `byohbuildingobjectexteriorscript::OnContainerChanged`, where the game writes
/// `cast ::temp5 ::mySelf_var` for `myself` — inherited from
/// `BYOHBuildingObjectScript`.
pub fn class_table(zip: &mut zip::ZipArchive<std::fs::File>, sources: &[String]) -> ClassTable {
    let parsed = parse_all(zip, sources);
    ClassTable::new(parsed.iter())
}

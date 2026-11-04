//! Walks the repository's `schemas` directory and writes the table of layers
//! the editor reads its schemas from.
//!
//! The schemas live outside this crate, in the repository root, because they
//! are data rather than code and are read by tools that are not Rust. The
//! build script reaches up to them and compiles what it finds into the
//! binary, so a release build is one file.

use std::path::{Path, PathBuf};
use std::{env, fs};

/// One layer of the schema chain: its name, and the files it holds.
///
/// The name is the directory's under `schemas` — `fnv`, `base` — and each file
/// is its bare filename beside its text. The layer a file belongs to is the
/// tuple's first element, not part of the filename, which is why the paths the
/// build script watches are tracked separately.
type Layer = (String, Vec<(String, String)>);

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    // `crates/nvre-schemas` -> the repository root.
    let repo = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("the repository root")
        .to_path_buf();
    let schemas = repo.join("schemas");
    // Cargo's `rerun-if-changed` on a **directory** only sees that directory's
    // own mtime, and rewriting a file inside it does not move it — so a schema
    // edit left cargo convinced the compiled-in table was current and the
    // release binary kept the old layouts. Emitting one directive per **file**
    // makes each of them a trigger, which is what an edit actually changes.
    //
    // The path has to be the file's real one, under its layer's directory: a
    // bare filename joined onto `schemas` names a file that does not exist, and
    // cargo treats a missing watched path as changed — which reruns the script
    // on *every* build, the opposite of the point.
    println!("cargo:rerun-if-changed={}", schemas.display());
    let (layers, watched) = collect(&schemas);
    for path in &watched {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let out = env::var("OUT_DIR").expect("OUT_DIR");
    let path = Path::new(&out).join("layers.rs");
    fs::write(&path, render(&layers)).expect("writing the layer table");
}

/// Every directory under `schemas` that holds schema files, as a layer, and the
/// real path of every file in it.
///
/// A directory is a layer when it holds at least one `.json` file. The
/// `base` symlink a game directory carries is not a layer of its own: it
/// points back at the base, which is already in the table, so following it
/// would merge the base twice.
///
/// The paths come back alongside the table because the build script needs both:
/// the table is layer name plus file name, and the watch directive needs the
/// file's actual location. Reconstructing one from the other loses the layer,
/// because the name in the table is the bare filename.
fn collect(schemas: &Path) -> (Vec<Layer>, Vec<PathBuf>) {
    let mut out = Vec::new();
    let mut watched = Vec::new();
    let mut dirs = vec![schemas.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut files = Vec::new();
        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            // A symlink is the `base` link a game directory carries, pointing
            // back at the base that is already in the table. Following it
            // would merge the base twice, so it is skipped; the real
            // directories are kept.
            if path.is_symlink() {
                continue;
            }
            if path.is_dir() {
                subdirs.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                let name = entry.file_name().to_string_lossy().into_owned();
                let text = fs::read_to_string(&path).unwrap_or_default();
                watched.push(path);
                files.push((name, text));
            }
        }
        if !files.is_empty() {
            files.sort_by(|a, b| a.0.cmp(&b.0));
            let name = layer_name(schemas, &dir);
            out.push((name, files));
        }
        dirs.extend(subdirs);
    }
    // Sorted, so the table is the same whatever order the walk found them in.
    out.sort_by(|a, b| a.0.cmp(&b.0));
    watched.sort();
    watched.dedup();
    (out, watched)
}

/// A directory's name under `schemas`, with `/` between the parts.
fn layer_name(schemas: &Path, dir: &Path) -> String {
    dir.strip_prefix(schemas)
        .unwrap_or(dir)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// The generated table, as Rust source.
fn render(layers: &[Layer]) -> String {
    let mut s = String::from("&[\n");
    for (name, files) in layers {
        s.push_str("    Layer {\n");
        s.push_str(&format!("        name: {name:?},\n"));
        s.push_str("        files: &[\n");
        for (file, text) in files {
            s.push_str(&format!("            ({file:?}, {text:?}),\n"));
        }
        s.push_str("        ],\n");
        s.push_str("    },\n");
    }
    s.push_str("]\n");
    s
}

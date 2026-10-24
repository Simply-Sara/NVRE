//! Walks the repository's `schemas` directory and writes the table of layers
//! the editor reads its schemas from.
//!
//! The schemas live outside this crate, in the repository root, because they
//! are data rather than code and are read by tools that are not Rust. The
//! build script reaches up to them and compiles what it finds into the
//! binary, so a release build is one file.

use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    // `crates/nvre-schemas` -> the repository root.
    let repo = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("the repository root")
        .to_path_buf();
    let schemas = repo.join("schemas");
    println!("cargo:rerun-if-changed={}", schemas.display());

    let layers = collect(&schemas);
    let out = env::var("OUT_DIR").expect("OUT_DIR");
    let path = Path::new(&out).join("layers.rs");
    fs::write(&path, render(&layers)).expect("writing the layer table");
}

/// Every directory under `schemas` that holds schema files, as a layer.
///
/// A directory is a layer when it holds at least one `.json` file. The
/// `base` symlink a game directory carries is not a layer of its own: it
/// points back at the base, which is already in the table, so following it
/// would merge the base twice.
fn collect(schemas: &Path) -> Vec<(String, Vec<(String, String)>)> {
    let mut out = Vec::new();
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
    out
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
fn render(layers: &[(String, Vec<(String, String)>)]) -> String {
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

# NVRE — New Vegas Rust Editor

A GECK-like editor for the Bethesda Gamebryo and Creation engine games —
Fallout: New Vegas, Fallout 3, Skyrim, Skyrim Special Edition and Fallout 4 —
written in Rust. Every one is read through its own schema layer, and every one
gets the same editor: the same panels, the same viewport, the same write path.
It loads a game's masters, archives, meshes and textures into a viewport where
objects can be placed, moved and deleted, then saves the result as a loadable
plugin.

NVRE is standalone: one cargo workspace, no upstream dependency, no submodule.
The game's files are only read; saving writes a new plugin wherever you ask.

## Run

```bash
cargo run -p nvre-editor
```


## Build

```bash
cargo build
```


## Layout

| Path | What it is |
|---|---|
| `crates/` | 15 crates: format readers (`nvre-esm`, `nvre-nif`, `bsa105`, `ba2`, `nvre-vfs`, `nvre-dds`), the world index (`nvre-world`), the write path (`nvre-edit`), the asset layer (`nvre-assets`), the obscript toolchain (`nvre-script`), and the GUI (`nvre-editor`). |
| `schemas/` | Record and NIF layouts, layered `base → family → version`. Each game loads only its own chain. Every layout is measured against the shipped masters, and carries a `confidence` and a `note` naming its source — see [schemas/README.md](schemas/README.md). |
| `docs/spec-notes/` | The measured notes behind the layouts: `fo4.md`, `fo4-csg-format.md`, `schema-coverage.md`. Every claim in `schemas/` traces to one of these. |
| `docs/obscript-bytecode.md` | The compiled-script format, measured against the games' own `SCTX`/`SCDA` pairs. |


## Scripts

`nvre-script` compiles obscript source (a `SCPT` record's `SCTX`) into the
bytecode the game runs (its `SCDA`), so a script can be edited without the GECK.
It is checked against the game itself: every shipped script carries both its
source and its compiled bytes, and the compiler reproduces **2,576 of 2,576** in
`FalloutNV.esm` and **1,256 of 1,257** in `Fallout3.esm` byte for byte.

```bash
NVRE_DATA=<Data> cargo run --release -p nvre-script --example parity_all
```

## Licence

GPL-3.0-or-later — see [LICENSE](LICENSE) and [LEGAL.md](LEGAL.md).

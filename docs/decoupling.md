# Where NVRE stands on upstream

NVRE does not use NVForever. Not as a git dependency, not as a patched
fork, not as a submodule, and not as a directory of copied source. The
tree has no reference to it outside the history of a comment.

## What replaced what

| was | is |
|---|---|
| `fnv-esm` (a fork) | `crates/nvre-esm` |
| `fnv-nif` (a fork) | `crates/nvre-nif` |
| `fnv-world` (a fork) | `crates/nvre-world` |
| `fnv-vfs` (a fork) | `crates/nvre-vfs` |
| `fnv-dds` (a fork) | `crates/nvre-dds` |
| `nvf-assets` (upstream, by git) | `crates/nvf-assets` |
| the `NVForever` submodule | nothing — it is gone |

The two crates that are forks of an upstream crate rather than new
crates — `nvre-vfs` and `nvre-dds` — are forks of *upstream's* code by
NVRE's own hand, and they carry no upstream dependency.

## The schema chain

The schemas are NVRE's own, and they are layered:

```
schemas/base   ->  what every supported game has, and whose layout decodes
                   correctly in all of them
schemas/fnv    ->  the Fallout family (Fallout: New Vegas and Fallout 3)
schemas/skyrim ->  the Skyrim family (Skyrim and Skyrim Special Edition)
schemas/skyrimle, schemas/skyrimse
                ->  each version's own differences
```

A record in a later directory replaces the one before it unless it says
`"merge": true`. Each game loads only the base, its family and its
version.

## What the readers measure

The ESM reader explains a game's masters, and the numbers are the
measure of that:

| game | subrecords | coverage |
|---|---|---|
| Fallout: New Vegas | 2,321,456 | 100.00% |
| Fallout 3 | 2,634,084 | 36.99% |
| Skyrim | 4,130,941 | 100.00% |
| Skyrim Special Edition | 4,131,332 | 100.00% |

Fallout 3's is a gap in the *schema*, not in the reader: the FO3 layer
does not describe every record the shipped masters carry. The reader and
the fork it replaced report the same number to the last subrecord.

Measure it with:

```bash
cargo build -p nvre-editor --bin esmcoverage
./target/debug/esmcoverage schemas/fnv "<fnv-data>/FalloutNV.esm"
```

The NIF reader is measured the same way, and the measure is stronger: a
file has to decode *and* re-encode to the bytes it was read from.

```bash
cargo build -p nvre-editor --bin nifprobe
NIF_LIMIT=3000 ./target/debug/nifprobe "<fnv-data>" schemas/nif/base
```

2,000 of Fallout: New Vegas's 20,542 NIF files — 62,669 blocks — decode
with no unknown block type, no short block and no error, and every one
re-encodes to the bytes it was read from.

## What the editor does

The editor runs on all four games with no upstream in the tree:

| game | records | cells | references | worldspaces |
|---|---|---|---|---|
| Fallout: New Vegas | 465,016 | 44,518 | 427,416 | 33 |
| Fallout 3 | 718,951 | 50,755 | 747,515 | 53 |
| Skyrim | 869,692 | 73,609 | 862,436 | 48 |
| Skyrim Special Edition | 869,687 | 73,725 | 878,404 | 49 |

```bash
NVRE_DATA="<game-data>" ./target/debug/nvre-editor --check --plugin "<game-data>/<master>.esm"
```

## What is left

- **Fallout 3's schema is incomplete.** 36.99% of its subrecords are
  described. The reader is not the problem; the FO3 layer is.
- **Skyrim Special Edition's archives are not read by the editor.** NVRE's
  own `bsa105` crate reads them — all 23 of the shipped ones, 172,961
  files — but the editor's VFS reads v104 only, so Skyrim SE opens with
  loose files only and says so.
- **Fallout 4 is not supported.** Its records are in BA2 archives, and its
  schemas are a different shape.

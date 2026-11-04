#!/usr/bin/env python3
"""Write a sectioned page for every record type, from its own subrecord list.

A page used to be Rust: 96 functions in `object_editor.rs`, each decoding Fallout:
New Vegas's byte offsets by hand. That works for one game and cannot work for
five — of those 96 page names, none is portable to all five games, 71 are the same
signature with a different layout, and 25 are record types Skyrim and Fallout 4 do
not have.

So the page for a record type is derived from the record type. Every subrecord the
schema says the record has goes into a heading, and the headings come from one
table of signature -> bucket that applies to every game at once. A record type no
one has looked at still gets a grouped page the day its schema parses, which is the
property that makes a new game cheap to add.

The bucket table is the only judgement in here, and it is deliberately coarse: a
signature whose meaning is obvious goes in its obvious bucket, and everything else
falls into "Other" — which is where a hand-edited page improves on this one.

**This is a bootstrapper, not an owner.** It writes a page only for a record type
that has no sections yet, so a page you have edited is never overwritten. Name a
type with `--only` (or pass `--force`) to regenerate one on purpose.

Usage:

    tools/pages_from_schema.py --game fnv          # report, change nothing
    tools/pages_from_schema.py --game fnv --write  # fill in the pages that have none
    tools/pages_from_schema.py --game fo4 --only WEAP --force --write  # redo one
"""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# Signature -> heading. Ordered: the first match wins, and the order is the order
# the headings appear in, so a record's page reads identity-first, damage-last.
#
# A signature can appear once. A record that does not carry a bucket's signatures
# simply has no such heading — which is what lets one table serve records whose
# layouts differ, and why a heading naming a subrecord only one game has is not an
# error.
BUCKETS = [
    ("Identity", """
        EDID FULL DESC CNAM FNAM NAM1 INAM RNAM ONAM WNAM TNAM UNAM VNAM SNAM
        XNAM YNAM ZNAM NAM0 NAM2 NAM3 NAM4 NAM5 NAM6 NAM7 NAM8 NAM9
        ETYP FLTR MODL MOD2 MOD3 MOD4 MOD5 MODS MODT ICON MICO ICO2 MIC2
        OBND DODT ONAM DATA2 BNAM PNAM QNAM
    """),
    ("Behaviour", """
        ACBS ACBS AIDT AIST AIPL AIFL CNAM TPLT VTCK RNAM PRKZ SPCT SPLO
        PKID PKW3 PKCU PSDC DNAM ANAM TNAM BNAM
    """),
    ("Combat", """
        DATA DNAM CRDT VATS DAMA MASE BIDS BAMT BMDT BMCT MODD MOSD INRD INNR
        ATKD ATKE ATKW ATKS ATKT AMMO EITM EAMT ENIT
    """),
    ("Keywords", "KSIZ KWDA FNAM"),
    ("Scripts", "VMAD SCRI SCPT SCPT"),
    ("Destruction", "DEST DSTD DSTF DMDL DMDT DSTT"),
    ("Sounds", "SOUN SNDD SNDX SNAM CNAM ANAM"),
    ("Magic", "EFID EFIT ETYP ENCH SPIT SPEL MGEF MDOB"),
    ("World", "XESP XSCL XRDS XOWN XCNT XLCN XTEL XTNM XMBO XPRM XLTW XLKR XACT"),
    ("Placement", "DATA NAME XTEL XESP XSCL XRDS XOWN XCNT"),
    ("Body", "BPTD BPND BPTN BPTP NAM1 NAM2 NAM3 NAM4 NAM5 SNAM TNAM"),
    ("Leveled", "LVLD LVLF LLCT LVLO COCT COED"),
    ("Containers", "COCT COED CNTO COCT DATA"),
    ("Conditions", "CTDA CITC CIS1 CIS2 CTDT"),
    ("Dialogue", "QSTI QSTR TIFC INDX QSDT CNAM TRDT NAM1 NAM2 RNAM ANAM DIAL INFO"),
    ("Regions", "RCLR RPLD RPLI RENT RDAT RDOT RDWT RDMD RDGS ICON MICO"),
    ("Weather", "NAM0 NAM1 FNAM CNAM INAM PNAM WNAM NAVM LNAM QNAM RNAM SNAM TNAM UNAM VNAM"),
    ("Navmesh", "NVMI NVNM NVER NVPP NVTR NVCA"),
]

# The heading for anything the table does not name.
FALLBACK = "Other"


def buckets():
    """The table, as signature -> heading, in heading order."""
    out = {}
    order = []
    for heading, sigs in BUCKETS:
        order.append(heading)
        for sig in sigs.split():
            # The first heading that names a signature owns it.
            out.setdefault(sig, heading)
    order.append(FALLBACK)
    return out, order


def sections_for(subrecords, table, order):
    """Group a record's subrecords into headings, keeping the record's own order."""
    grouped = {h: [] for h in order}
    for sig in subrecords:
        grouped[table.get(sig, FALLBACK)].append(sig)
    return [
        {"title": h, "subs": subs} for h, subs in grouped.items() if subs
    ]


def load_schema(game):
    """Every record type the game's *loaded* schema holds, from `dump_subs`.

    Not re-read from the JSON files: the chain is `base -> family -> version` and a
    later layer replaces a record whole unless it says `merge`. A tool that
    re-implemented that rule would be a second copy of it, and the two would drift
    the first time a layer changed. The Rust side already loads the schema; this
    asks it.
    """
    out = subprocess.run(
        [
            "cargo", "run", "-q", "--example", "dump_subs", "-p", "nvre-editor", "--",
            "--game", game,
        ],
        cwd=REPO,
        capture_output=True,
        text=True,
        env={**os.environ, "CARGO_TARGET_DIR": "/home/sara/Projects/NVRE/target",
             "RUSTC_WRAPPER": ""},
    )
    if out.returncode != 0:
        raise SystemExit(f"dump_subs failed: {out.stderr[-800:]}")
    records = {}
    for line in out.stdout.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        row = json.loads(line)
        records[row["sig"]] = row["subs"]
    return records


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True,
                    choices=["fnv", "fo3", "skyrim", "skyrimse", "fo4"])
    ap.add_argument("--write", action="store_true")
    ap.add_argument("--force", action="store_true",
                    help="overwrite pages that already have sections")
    ap.add_argument("--only", action="append", default=None,
                    help="restrict to these signatures (repeatable)")
    args = ap.parse_args()

    table, order = buckets()
    records = load_schema(args.game)
    path = REPO / "schemas" / args.game / "object_window.json"
    if not path.exists():
        print(f"{args.game}: no object_window.json; nothing to write", file=sys.stderr)
        return 1
    doc = json.loads(path.read_text())
    cats = doc.setdefault("object_window", {}).setdefault("categories", {})
    only = set(args.only or [])

    written, kept = [], 0
    for sig, subs in sorted(records.items()):
        if only and sig not in only:
            continue
        cat = cats.setdefault(sig, {"group": "Miscellaneous"})
        # A page that already has sections was written by someone — an earlier run,
        # or a person improving the headings. Overwriting it would throw that work
        # away, which is the opposite of the point of moving pages into data. It is
        # kept unless the run names the type or says `--force`.
        if cat.get("sections") and not args.force and sig not in only:
            kept += 1
            continue
        cat["sections"] = sections_for(subs, table, order)
        written.append(sig)

    if args.write:
        path.write_text(json.dumps(doc, indent=1, sort_keys=True) + "\n")
        print(
            f"wrote {path}: {len(written)} record types given sections, "
            f"{kept} kept as they were",
            file=sys.stderr,
        )
    else:
        print(
            f"would write {len(written)} record types, keep {kept}"
            + (f": {', '.join(written[:8])}{'...' if len(written) > 8 else ''}"
               if written else ""),
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())

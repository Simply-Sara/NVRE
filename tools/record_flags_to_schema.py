#!/usr/bin/env python3
"""Extract each record's header-flag list from xEdit's definitions.

The record header's `Record Flags` word is a u32 whose named bits differ per
game *and per record type*: bit 10 is "Quest Item" in Fallout's records, bit 6 is
"Shield" on armour, bit 2 is "Non-Playable", and Skyrim names bit 15 "Visible
When Distant" where Fallout 4 leaves it unknown. That is the one piece of a
record's header the schema could not answer, and the object editor hardcoded
Fallout: New Vegas's bit 10 for every game.

xEdit declares the list as the second argument of `wbRecord`:

    wbRecord(WEAP, 'Weapon',
      wbFlags(wbFlagsList([
        10, 'Quest Item',
        27, 'Unknown 27'
      ])), [

so it is read here rather than retyped. The output is a schema fragment in the
`header_flags` shape `nvre_esm::Schema` parses:

    { "format": "fnv.schema/1",
      "header_flags": { "WEAP": { "bits": { "10": "Quest Item" }, ... } } }

Usage:

    tools/record_flags_to_schema.py --game fnv    > schemas/fnv/header_flags.json
    tools/record_flags_to_schema.py --game fo3    > schemas/fo3/header_flags.json
    tools/record_flags_to_schema.py --game skyrim > schemas/skyrim/header_flags.json
    tools/record_flags_to_schema.py --game fo4    > schemas/fo4/header_flags.json

A record whose flags argument is not an inline list (xEdit passes a variable, or
omits the argument entirely) is reported on stderr and left out, so a gap is
visible rather than silently empty.
"""

import argparse
import json
import re
import sys

XEDIT = "/home/sara/TES5Edit/Core"

# Which xEdit file defines which game, and which schema format it writes.
GAMES = {
    "fnv": ("wbDefinitionsFNV.pas", "fnv.schema/1"),
    "fo3": ("wbDefinitionsFO3.pas", "fnv.schema/1"),
    "skyrim": ("wbDefinitionsTES5.pas", "fnv.schema/1"),
    "fo4": ("wbDefinitionsFO4.pas", "fnv.schema/1"),
}

# `wbRecord(SIG, 'Name',` — the flags argument follows the name.
RECORD_RE = re.compile(r"wbRecord\(\s*([A-Z0-9_]{4})\s*,\s*'([^']*)'\s*,")
# The inline flag list. xEdit nests it as wbFlags(wbFlagsList([ ... ])), and the
# closing `]`/`)` is the end of the *list*: a record may add required-flag
# arguments after it (`wbFlags(wbFlagsList([...]), [17])`), so the match must
# stop at the list's own bracket rather than looking for the outer call's `)`.
FLAGS_RE = re.compile(r"\s*wbFlags\(\s*wbFlagsList\(\[(.*?)\]\s*\)", re.S)
# One `bit, 'name',` pair. Pascal escapes a quote inside a string by doubling it
# ("Can''t Wait"), so the name may contain `''`.
PAIR_RE = re.compile(r"(\d+)\s*,\s*'((?:[^']|'')*)'")

# Bits xEdit does not name, from a source that does: `{game: {sig: ({bit: name},
# citation)}}`. Adding one here rather than to the generated file is what makes
# the file reproducible.
HAND_ADDED = {
    "fnv": {
        "IMOD": (
            {10: "Quest Item"},
            "GECK wiki IsQuestItem / SetQuestObject [owner export 2026-10-07]: "
            "Weapon Mod (103) is among the form types that can be quest items, "
            "and GetFormFlag gives bit 10 (1024) as 'Quest item'. xEdit gives "
            "IMOD no record flag list, so the bit is unnamed there.",
        ),
    },
}


def strip_comments(text: str) -> str:
    """Remove Pascal comments so a commented-out flag is not read as a flag."""
    text = re.sub(r"\{[^}]*\}", "", text)
    text = re.sub(r"\(\*.*?\*\)", "", text, flags=re.S)
    text = re.sub(r"//[^\n]*", "", text)
    return text


def parse(path: str):
    """Every record's flag list, by signature."""
    with open(path, encoding="utf-8", errors="replace") as f:
        text = strip_comments(f.read())
    out = {}
    skipped = []
    for m in RECORD_RE.finditer(text):
        sig, name = m.group(1), m.group(2)
        rest = text[m.end():]
        fm = FLAGS_RE.match(rest)
        if not fm:
            # Either no flags argument at all, or one xEdit names elsewhere.
            skipped.append((sig, name))
            continue
        # A list that runs into another record's declaration is a mis-parse: it
        # means the record's own flags argument is not the inline form and the
        # match found a later one. Reading it would attribute another record's
        # flags to this one, so it is reported instead.
        if "wbRecord(" in fm.group(1):
            skipped.append((sig, name))
            continue
        bits = {}
        for pm in PAIR_RE.finditer(fm.group(1)):
            bit = int(pm.group(1))
            label = pm.group(2).replace("''", "'")
            bits[str(bit)] = label
        out[sig] = {"name": name, "bits": bits}
    return out, skipped


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, choices=sorted(GAMES))
    ap.add_argument("--indent", type=int, default=1)
    args = ap.parse_args()

    filename, fmt = GAMES[args.game]
    path = f"{XEDIT}/{filename}"
    records, skipped = parse(path)

    entries: dict[str, dict] = {
        sig: {
            "confidence": "documented",
            "bits": {str(bit): label for bit, label in rec["bits"].items()},
        }
        for sig, rec in records.items()
    }
    # A few records have a flag xEdit does not name and another source does.
    # They are carried here rather than hand-written into the output so that
    # regenerating the file keeps them, and so that the citation travels with
    # the bit.
    for sig, (bits, note) in HAND_ADDED.get(args.game, {}).items():
        entry = entries.setdefault(sig, {"bits": {}})
        for bit, label in bits.items():
            entry["bits"][str(bit)] = label
        entry["note"] = note
    # Numeric bit order, because a dict sorted as text puts "10" before "2".
    for entry in entries.values():
        entry["bits"] = dict(sorted(entry["bits"].items(), key=lambda kv: int(kv[0])))

    doc = {
        "format": fmt,
        "_comment": (
            f"Record header flags, from xEdit's {filename} "
            f"(`wbRecord(<SIG>, '<Name>', wbFlags(wbFlagsList([...])))`). "
            "Each record's `Record Flags` word is a u32, and which bits have "
            "names differs per game and per record type: bit 10 is 'Quest Item' "
            "on a Fallout weapon and unnamed on Fallout 4's, and Skyrim names "
            "bit 0 of a CELL where Fallout does not. The object editor renders "
            "the header from this table rather than from a constant, so a "
            "record type xEdit names no flags for has an empty entry and shows "
            "the raw word only. "
            "Generated by tools/record_flags_to_schema.py; do not hand-edit. "
            "Entries carrying a `note` hold a bit from a source other than "
            "xEdit, cited in the note."
        ),
        "header_flags": dict(sorted(entries.items())),
    }
    json.dump(doc, sys.stdout, indent=args.indent)
    sys.stdout.write("\n")
    with_flags = sum(1 for r in entries.values() if r["bits"])
    print(
        f"{args.game}: {len(entries)} records, {with_flags} with a named flag "
        f"list, {len(skipped)} without an inline list",
        file=sys.stderr,
    )
    if skipped:
        print("  no inline flag list: " + " ".join(s for s, _ in skipped), file=sys.stderr)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Add xEdit's FormID target lists to a schema file, without touching anything else.

A FormID field is not just "a reference": xEdit says what it may point at.

    wbFormIDCk(INAM, 'Inventory Art', [STAT])

That list is the difference between a picker that offers 44,550 objects and one
that offers the statics. `wbDefinitionsFO4.pas` declares it 603 times, and the
schema used to drop it, leaving a bare `"formid"`.

This rewrites **only** those. Every other byte of the file is preserved: the
script loads the JSON, replaces `"formid"` with `{"formid": {"of": [...]}}` where
the Pascal names targets, and writes it back. It is deliberately not a
regeneration — the checked-in `schemas/fo4/generated.json` is not reproducible
from `tools/fo4_pascal_to_schema.py` today (that tool produces 113 types where
the file has 155, and omits 7 types its own records reference), so regenerating
would lose information. Adding what is missing cannot.

Usage:

    tools/formid_targets.py --pascal /home/sara/TES5Edit/Core/wbDefinitionsFO4.pas \
                            --schema schemas/fo4/generated.json --write

Without `--write` it reports what it would change.
"""

import argparse
import json
import re
import sys
from pathlib import Path

# `wbRecord(SIG, 'Name',` opens a record; its body runs to the next one.
RECORD_RE = re.compile(r"wbRecord\(\s*([A-Z0-9_]{4})\s*,")
# A checked FormID: `wbFormIDCk(INAM, 'Name', [STAT, NULL])`.
FORMID_RE = re.compile(
    r"wbFormID(?:Ck|CkNoReach|CK|ck)\(\s*([A-Z0-9]{4})\s*,\s*'((?:[^']|'')*)'\s*,\s*\[([^\]]*)\]",
)


def strip_comments(text):
    text = re.sub(r"\{[^}]*\}", "", text)
    text = re.sub(r"\(\*.*?\*\)", "", text, flags=re.S)
    text = re.sub(r"//[^\n]*", "", text)
    return text


def record_bodies(pascal):
    """Each record's body, by signature. A record's body is up to the next one."""
    starts = [(m.start(), m.group(1)) for m in RECORD_RE.finditer(pascal)]
    out = {}
    for i, (pos, sig) in enumerate(starts):
        end = starts[i + 1][0] if i + 1 < len(starts) else len(pascal)
        # A signature can appear twice (a record redefined in a branch); the
        # last definition wins, which is what xEdit's own loader does.
        out[sig] = pascal[pos:end]
    return out


def targets_in(body, sub_sig):
    """The record types xEdit says a record's `sub_sig` field may point at."""
    for m in FORMID_RE.finditer(body):
        if m.group(1) != sub_sig:
            continue
        out = []
        for t in re.findall(r"\b([A-Z0-9]{4})\b", m.group(3)):
            # `NULL` is "no value", which every FormID already allows.
            if t != "NULL" and t not in out:
                out.append(t)
        return out
    return None


def walk(node, rec_sig, bodies, stats):
    """Replace targeted `"formid"` leaves in one record's definition.

    `node` is a mapping of subrecord signature to definition, or a `type` value
    inside one. A definition is recognised by its `type` being `"formid"` and its
    key being a four-byte signature — the key, not the `name`, because the
    generated definitions carry the signature as the key and a descriptive name
    (or the signature again) in `name`.
    """
    if isinstance(node, dict):
        for k, v in list(node.items()):
            if isinstance(v, dict) and v.get("type") == "formid" and len(k) == 4:
                tags = targets_in(bodies[rec_sig], k)
                if tags:
                    v["type"] = {"formid": {"of": tags}}
                    stats["targeted"] += 1
                else:
                    stats["untargeted"] += 1
                continue
            walk(v, rec_sig, bodies, stats)
    elif isinstance(node, list):
        for v in node:
            walk(v, rec_sig, bodies, stats)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pascal", required=True)
    ap.add_argument("--schema", required=True)
    ap.add_argument("--write", action="store_true")
    args = ap.parse_args()

    pascal = strip_comments(Path(args.pascal).read_text(encoding="utf-8", errors="replace"))
    bodies = record_bodies(pascal)
    path = Path(args.schema)
    doc = json.loads(path.read_text())

    stats = {"targeted": 0, "untargeted": 0, "records": 0}
    for sig, rec in (doc.get("records") or {}).items():
        if sig not in bodies:
            continue
        stats["records"] += 1
        # The subrecord signatures are the keys under `subrecords`, and a
        # sequence's blocks have their own. `name` on each definition is what
        # xEdit declares, so the match is on the signature the definition names.
        walk(rec.get("subrecords") or {}, sig, bodies, stats)
        for block in ((rec.get("sequence") or {}).get("blocks") or {}).values():
            walk(block.get("subrecords") or {}, sig, bodies, stats)

    print(
        f"{args.schema}: {stats['targeted']} FormIDs given targets, "
        f"{stats['untargeted']} left unrestricted, over {stats['records']} records",
        file=sys.stderr,
    )
    if args.write:
        path.write_text(json.dumps(doc, indent=1, sort_keys=True) + "\n")
        print(f"wrote {args.schema}", file=sys.stderr)


if __name__ == "__main__":
    main()

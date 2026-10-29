#!/usr/bin/env python3
"""Build the Fallout 4 schema layer from nif.xml.

The base layer holds what every game has; this layer holds Fallout 4's
own layouts, which is what the game's blocks need.

The layouts come from Bethesda's nif.xml, converted by
`nifxml_to_schema.py`. The hand-written notes in the layer are kept, so
that a reader of the file still knows why a type is the way it is.

Only Fallout 4 loads this layer, so it holds the whole of Fallout 4's
schema: a type the base layer has and this one does not is left to the
base, and a type both have is this one's.

Usage:
    build_fo4_layer.py <generated.json> <current.json> <out.json>
"""

from __future__ import annotations

import json
import sys


def main():
    gen = json.load(open(sys.argv[1]))
    cur = json.load(open(sys.argv[2]))
    out_path = sys.argv[3]

    # The notes the hand-written layer has, so that a reader of the file
    # still knows why a type is the way it is.
    notes = {}
    for section in ("blocks", "structs"):
        for name, v in cur.get(section, {}).items():
            if v.get("note"):
                notes[(section, name)] = v["note"]

    blocks = {}
    for name, v in gen["blocks"].items():
        key = ("blocks", name)
        if key in notes:
            v = dict(v)
            v["note"] = notes[key]
        blocks[name] = v

    structs = {}
    for name, v in gen["structs"].items():
        key = ("structs", name)
        if key in notes:
            v = dict(v)
            v["note"] = notes[key]
        structs[name] = v

    # The types the hand-written layer has that the generated one does
    # not: they are kept, because they are not in the base either.
    for section, out in (("blocks", blocks), ("structs", structs)):
        for name, v in cur.get(section, {}).items():
            if name not in out:
                out[name] = v

    doc = {
        "format": "fnv.nif.schema/1",
        "_comment": cur.get(
            "_comment",
            "Fallout 4's own NIF layouts, layered over schemas/nif/base.",
        ),
        "structs": structs,
        "blocks": blocks,
    }
    # The enums and bit flags, which the generated schema carries and the
    # hand-written one did not.
    if gen.get("enums"):
        doc["enums"] = gen["enums"]
    with open(out_path, "w") as fh:
        json.dump(doc, fh, indent=2, sort_keys=True)
        fh.write("\n")
    print(
        f"wrote {out_path}: {len(blocks)} blocks, {len(structs)} structs, "
        f"{len(gen.get('enums', {}))} enums"
    )


if __name__ == "__main__":
    main()

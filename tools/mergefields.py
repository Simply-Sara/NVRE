#!/usr/bin/env python3
"""Merge the fields of a record as the schema actually reads them.

A schema directory is read in path order, and a record in a later file
replaces the earlier one outright — the earlier record's fields are gone,
not merged. So a per-game record has to carry every field the earlier one
had, or those fields stop decoding.

This walks the files in the order the schema reads them, keeps the
accumulated field set for each record, and writes the merged result into
one output file. The merged record keeps the *last* definition of each
field, so a measured layout wins over a generated one.

    tools/mergefields.py schemas/base,schemas/fnv,schemas/skyrimse > merged.json

Records to leave alone can be named after `--skip`:

    tools/mergefields.py --skip SCEN --skip QUST schemas/base,schemas/skyrimse
"""
import collections
import json
import os
import sys


def files_in_order(dirs):
    out = []
    for d in dirs:
        for name in sorted(os.listdir(d)):
            path = os.path.join(d, name)
            if os.path.isfile(path) and name.endswith(".json"):
                out.append(path)
    return out


def main():
    argv = sys.argv[1:]
    skip = set()
    while argv and argv[0] == "--skip":
        skip.add(argv[1])
        argv = argv[2:]
    dirs = argv[0].split(",")

    records = collections.OrderedDict()
    types = collections.OrderedDict()
    for path in files_in_order(dirs):
        try:
            j = json.load(open(path))
        except Exception as e:
            print(f"skipping {path}: {e}", file=sys.stderr)
            continue
        for name, t in j.get("types", {}).items():
            types[name] = t
        for sig, r in j.get("records", {}).items():
            if sig in skip:
                continue
            cur = records.setdefault(sig, {"name": r.get("name", ""), "subrecords": {}})
            if r.get("name"):
                cur["name"] = r["name"]
            for k, v in r.get("subrecords", {}).items():
                cur["subrecords"][k] = v
            for k in ("header_flags", "header_flags_confidence", "sequence"):
                if k in r:
                    cur[k] = r[k]
            if "note" in r:
                cur["note"] = r["note"]
            if "confidence" in r:
                cur["confidence"] = r["confidence"]

    out = {"format": "fnv.schema/1", "records": records}
    if types:
        out["types"] = types
    json.dump(out, sys.stdout, indent=1, sort_keys=True)


if __name__ == "__main__":
    main()

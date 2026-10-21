#!/usr/bin/env python3
"""Split a schema directory into a shared layer and a version layer.

Skyrim LE and Skyrim SE are the same game at two versions, and they share
almost every record layout. A schema directory that holds both versions'
definitions in one place has to restate a layout to change it, and the
restatement drifts. This splits a directory into the layouts both versions
use and the ones only one version uses, so a version layer holds only its
own differences.

    tools/splitlayer.py schemas/skyrimse schemas/skyrim schemas/skyrimse
"""
import collections
import json
import os
import sys


def load(dirname):
    """The directory's files, in the order the schema reads them."""
    out = []
    for name in sorted(os.listdir(dirname)):
        path = os.path.join(dirname, name)
        if not os.path.isfile(path) or not name.endswith(".json"):
            continue
        if os.path.islink(path):
            # A symlink to the base, which the loader skips.
            continue
        out.append((name, json.load(open(path))))
    return out


def named(t, out):
    """Every type name a type expression refers to."""
    if isinstance(t, str):
        out.add(t)
        return
    if not isinstance(t, dict):
        return
    for k, v in t.items():
        if k in ("type", "count_field", "switch_on"):
            named(v, out)
        elif k == "struct" and isinstance(v, list):
            for f in v:
                named(f.get("type") if isinstance(f, dict) else None, out)
        elif k == "array":
            named(v.get("elem") if isinstance(v, dict) else None, out)
        elif k == "union" and isinstance(v, list):
            for x in v:
                named(x.get("type") if isinstance(x, dict) else None, out)
        elif k == "peers":
            for x in v:
                named(x, out)


def named_types(sd):
    """Every type name a subrecord definition refers to."""
    out = set()
    named(sd.get("type"), out)
    return out


def record_types(rec):
    """Every type name a record's subrecords and sequence blocks refer to."""
    out = set()
    for sd in rec.get("subrecords", {}).values():
        named(sd.get("type"), out)
    seq = rec.get("sequence")
    if seq:
        for block in seq.get("blocks", {}).values():
            for sd in block.get("subrecords", {}).values():
                named(sd.get("type"), out)
            for var in block.get("variants", {}).values():
                for sd in var.get("subrecords", {}).values():
                    named(sd.get("type"), out)
    return out


def main():
    src, shared_dir, version_dir = sys.argv[1], sys.argv[2], sys.argv[3]
    # The records that differ between the two versions, measured over both
    # masters. A record that differs is a version record, not a shared one.
    differ = set(sys.argv[4:])

    files = load(src)
    # The version layer starts empty: the split writes into it, and a file
    # left from an earlier run would load a record twice.
    for name in sorted(os.listdir(version_dir)):
        path = os.path.join(version_dir, name)
        if os.path.isfile(path) and name.endswith(".json") and not os.path.islink(path):
            os.remove(path)
    all_types = {}
    for name, j in files:
        all_types.update(j.get("types", {}))

    shared_types = set()
    for name, j in files:
        for rec in j.get("records", {}).values():
            shared_types |= record_types(rec)
        # The common map is part of the shared layer's layout: a signature
        # many records share is the game's own, and both versions use it.
        for sd in j.get("common", {}).values():
            shared_types |= named_types(sd)
    # A type belongs to the shared layer when the shared layer reaches it:
    # a shared record or a shared common entry names it, directly or through
    # another shared type. The closure is taken over the whole type table, so
    # a type named only by another type still lands in the right layer.
    changed = True
    while changed:
        changed = False
        for t, tv in all_types.items():
            if t in shared_types:
                continue
            refs = set()
            named(tv, refs)
            if refs & shared_types:
                shared_types.add(t)
                changed = True

    os.makedirs(shared_dir, exist_ok=True)
    os.makedirs(version_dir, exist_ok=True)
    shared_types_out = {}
    version_types_out = {}
    for name, j in files:
        shared_recs = {s: r for s, r in j.get("records", {}).items() if s not in differ}
        version_recs = {s: r for s, r in j.get("records", {}).items() if s in differ}
        types = j.get("types", {})
        j.pop("types", None)
        # A file with no records in a layer is not written to that layer, and
        # the file it came from is removed: leaving it behind would load the
        # same record twice.
        # A file's common map belongs to the layer that owns its types. The
        # common map is the game's own layout for a signature that many
        # records share, so it travels with the types it names.
        common = j.get("common", {})
        common_types = set()
        for sd in common.values():
            named(sd.get("type"), common_types)
        if common and common_types & shared_types:
            shared_common = {
                k: v
                for k, v in common.items()
                if named_types(v) & shared_types
            }
            if shared_common:
                j["common"] = shared_common
                json.dump(
                    {"format": "fnv.schema/1", "common": shared_common},
                    open(os.path.join(shared_dir, name), "w"),
                    indent=1,
                    sort_keys=True,
                )
            rest = {k: v for k, v in common.items() if k not in shared_common}
            if rest:
                json.dump(
                    {"format": "fnv.schema/1", "common": rest},
                    open(os.path.join(version_dir, name), "w"),
                    indent=1,
                    sort_keys=True,
                )
        else:
            for target, recs in ((shared_dir, shared_recs), (version_dir, version_recs)):
                if not recs and not common:
                    continue
                j["records"] = recs
                if common:
                    j["common"] = common
                json.dump(j, open(os.path.join(target, name), "w"), indent=1, sort_keys=True)
        for t, tv in types.items():
            if t in shared_types:
                shared_types_out[t] = tv
            else:
                version_types_out[t] = tv
    if shared_types_out:
        json.dump(
            {"format": "fnv.schema/1", "types": shared_types_out},
            open(os.path.join(shared_dir, "types.json"), "w"),
            indent=1,
            sort_keys=True,
        )
    if version_types_out:
        json.dump(
            {"format": "fnv.schema/1", "types": version_types_out},
            open(os.path.join(version_dir, "types.json"), "w"),
            indent=1,
            sort_keys=True,
        )
    print(
        f"the shared layer: {len(shared_types_out)} types\n"
        f"the version layer: {len(version_types_out)} types"
    )


if __name__ == "__main__":
    main()

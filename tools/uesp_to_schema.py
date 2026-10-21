#!/usr/bin/env python3
"""Turn the UESP field tables into NVRE schema JSON.

The UESP pages for a Skyrim record list its fields in a table: a marker column
saying whether the field is required, optional or repeated, the signature, the
name, the type, and a note. This reads that table and writes the records and
subrecords, leaving the type to a small mapping.

It is a starting point, not the finished schema: every generated entry is
marked `hypothesis` and carries the UESP page as its source, so a human can
see what was taken from the documentation and what still needs checking
against the shipped data.
"""

import json
import re
import sys
from pathlib import Path

SCRATCH = Path(
    __import__("os").environ.get(
        "NVRE_UESP_SCRATCH",
        "/home/sara/.hermes/profiles/fallout-nv-modding/cache/scratch",
    )
)

# UESP's type names to NVRE's schema types.
TYPES = {
    "zstring": "zstring",
    "lstring": "u32",
    "string": "zstring",
    "formID": "formid",
    "formid": "formid",
    "uint8": "u8",
    "uint16": "u16",
    "uint32": "u32",
    "uint64": "u64",
    "int8": "i8",
    "int16": "i16",
    "int32": "i32",
    "int64": "i64",
    "ubyte": "u8",
    "byte": "u8",
    "float": "f32",
    "float32": "f32",
    "double": "f64",
    "struct": "bytes",
    "struct[]": "bytes",
    "struct[8]": "bytes",
    "bool": "u8",
}

# The fields whose type is a named struct rather than a primitive. These are
# written out by hand in the schema files, so the generator skips them.
NAMED = {
    "Condition", "CTDA", "OBND", "ObjectBounds", "VMAD", "MODL", "DEST",
    "MODT", "MODB", "MODS", "MODD", "LVLO", "MHDT",
}

# A signature that is not really a field but a section marker.
SKIP_SIGS = {"", "-"}


def parse_table(text):
    """The rows of the field table, as (marker, sig, name, type, note)."""
    rows = []
    # The table's rows start with a marker character and a tab.
    for line in text.splitlines():
        if not line or line[0] not in "+-*":
            continue
        parts = line.split("\t")
        if len(parts) < 4:
            continue
        marker = parts[0].strip()
        # Some pages put a version range in its own column, such as "29+".
        if re.fullmatch(r"\d+\+?", parts[1].strip()):
            parts = [parts[0]] + parts[2:]
        # Some pages have a "V" column between the marker and the field.
        if parts[1].strip() in {"V", ""} and len(parts) > 4:
            parts = [parts[0]] + parts[2:]
        sig = parts[1].strip()
        name = parts[2].strip()
        typ = parts[3].strip()
        note = parts[4].strip() if len(parts) > 4 else ""
        if not sig or not re.fullmatch(r"[A-Z0-9]{4}", sig):
            continue
        rows.append((marker, sig, name, typ, note))
    return rows


# Fields whose element count is a count field elsewhere in the same record.
# UESP writes them as `formID[KSIZ.keywordCount]`, and the schema expresses
# that as an array whose count is a sibling field.
COUNT_FIELD_ARRAYS = {
    "KWDA": "KSIZ",   # keywords
    "LVLO": "LLCT",   # leveled item entries
    "NNAM": "QNAM",   # quest nodes
}


def type_of(typ, note):
    """The schema type for a UESP type and note."""
    base = typ.split("[")[0].strip()
    # A formID array is a run of formids with no count of its own.
    if typ.strip() == "formID[]":
        return {"array": "formid"}
    if base in ("formID", "formid"):
        return "formid"
    if base == "zstring":
        return "zstring"
    if base in ("uint8", "ubyte", "byte", "int8", "bool"):
        return "u8" if base != "int8" else "i8"
    if base in ("uint16", "int16"):
        return "u16" if base == "uint16" else "i16"
    if base in ("uint32", "int32"):
        return "u32" if base == "uint32" else "i32"
    if base in ("uint64", "int64"):
        return "u64" if base == "uint64" else "i64"
    if base in ("float", "float32"):
        return "f32"
    if base == "double":
        return "f64"
    if base == "lstring":
        return "u32"
    if base.startswith("struct"):
        return "bytes"
    # Some pages put the field's own name in the type column rather than a
    # type. KSIZ is always the keyword count, a uint32.
    if base == "KSIZ":
        return "u32"
    return "bytes"


# The records whose structure the field tables do not give, and the few whose
# types the tables only name. They are written by hand, and the generator
# leaves them alone.
SKIPPED_RECORDS = {
    "SCEN", "PACK", "QUST", "DIAL", "INFO", "RACE", "NPC_",
    "WEAP", "ACHR", "LVLI", "ACTI", "IDLE", "SMQN", "WTHR", "SNDR",
    "NAVM", "IPDS", "DLBR", "DLVW", "WRLD", "CELL", "REFR", "ARMO", "STAT",
    "FACT",
}


def main():
    out_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else SCRATCH
    records = {}
    for path in sorted(SCRATCH.glob("*.txt")):
        rec = path.stem
        if rec in SKIPPED_RECORDS:
            continue  # already written by hand
        text = path.read_text()
        rows = parse_table(text)
        if not rows:
            continue
        subs = {}
        for marker, sig, name, typ, note in rows:
            if sig in SKIP_SIGS or sig in NAMED:
                continue
            field_type = type_of(typ, note)
            entry = {
                "name": name or sig,
                "confidence": "hypothesis",
                "note": f"UESP, Skyrim Mod:Mod File Format/{rec}. {note}".strip(),
                "type": field_type,
            }
            if sig in COUNT_FIELD_ARRAYS:
                count_from = COUNT_FIELD_ARRAYS[sig]
                if "count_field" in str(field_type):
                    # The count is a field of the same struct.
                    entry["type"]["count_field"] = count_from
                else:
                    # The count is a sibling subrecord, so the array takes it
                    # from there rather than from the data in front of it.
                    entry["type"] = {"array": field_type}
                    entry["count_from"] = count_from
            subs.setdefault(sig, entry)
        if subs:
            records[rec] = {
                "name": rec,
                "confidence": "hypothesis",
                "note": f"Generated from the UESP field table for {rec}; every entry still needs checking against the shipped data.",
                "subrecords": subs,
            }
    out = {"format": "fnv.schema/1", "records": records}
    dest = out_dir / "generated.json"
    dest.write_text(json.dumps(out, indent=1, sort_keys=True))
    print(f"{dest}: {len(records)} records")

    # The hand-written files carry the records whose structure the tables do
    # not give, and a few whose types the tables only name. They are listed
    # here so a run of this script does not silently drop one.
    derived = set(records)
    print(f"note: {len(SKIPPED_RECORDS)} records are written by hand")


if __name__ == "__main__":
    main()

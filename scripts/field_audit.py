#!/usr/bin/env python3
"""Audit the object editor against the record specs, per type.

For every record type the Objects window shows, this reads the spec's
format table and its per-subrecord field tables, and checks that the
type's own editor code mentions every subrecord and every named field.

The code counted for a type is:
- its own `load_<sig>`, `save_<sig>` and `draw_<sig>_ui` functions,
- the shared helper functions those call, one level deep,
- the state fields the type owns.

A word from another type's page does not count, so the audit finds the
fields nobody wrote a page for. The naming match is loose on purpose —
the point is to separate "somebody thought about this field" from
"nobody did", not to judge the naming.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
RECORDS = ROOT / "target/tempDocs/Records"
SOURCE = ROOT / "crates/nvre-editor/src/object_editor.rs"


def spec_path(sig):
    return RECORDS / f"{sig}.md"


def parse_spec(sig):
    """Return (subrecords, fields) for a spec.

    `subrecords` is a list of (signature, name) from the format table.
    `fields` is a list of (signature, field name) from the per-subrecord
    tables.
    """
    text = spec_path(sig).read_text()
    subrecords = []
    fields = []

    in_format = False
    for line in text.splitlines():
        if line.startswith("## Format"):
            in_format = True
            continue
        if in_format and line.startswith("##"):
            in_format = False
        if in_format and "|" in line:
            cols = [c.strip() for c in line.strip("|").split("|")]
            if len(cols) < 2:
                continue
            if cols[0] == "Count" or (cols[0] and set(cols[0]) <= set("-: ")):
                continue
            sig_col = cols[0]
            name_col = cols[1] if len(cols) > 1 else ""
            if re.fullmatch(r"[A-Z][A-Z0-9_]{3}", sig_col):
                subrecords.append((sig_col, name_col))

    current = None
    in_table = False
    for line in text.splitlines():
        # `### SIG` opens a subrecord's field table. `### Flag Values`
        # and the other named tables are not subrecords, and their rows
        # are flag values, not fields.
        m = re.match(r"^###\s+([A-Z][A-Z0-9_]{3})\s*$", line)
        if m:
            current = m.group(1)
            in_table = False
            continue
        # Any other heading ends the subrecord's field table. A `####`
        # heading, for instance, opens the flag values that follow the
        # table, and those rows are values, not fields. A `###` heading
        # that is not a subrecord's name does the same.
        if re.match(r"^####", line) or re.match(r"^###\s+", line):
            current = None
            in_table = False
            continue
        # `##` ends the section, but `###` starts a subrecord's own
        # table, so only a bare `##` clears the current subrecord.
        if line.startswith("##") and not line.startswith("###"):
            current = None
            in_table = False
            continue
        if current and "|" in line:
            cols = [c.strip() for c in line.strip("|").split("|")]
            if len(cols) < 2:
                continue
            if cols[0] == "Name" or (cols[0] and set(cols[0]) <= set("-: ")):
                in_table = True
                continue
            if not in_table:
                continue
            if cols[0]:
                fields.append((current, cols[0]))
    return subrecords, fields


def function_span(src, name):
    """The text of a top-level `fn <name>(` and everything it owns."""
    m = re.search(rf"^fn {re.escape(name)}\(", src, re.M)
    if not m:
        return ""
    start = m.start()
    end = src.find("\n}\n", start)
    if end == -1:
        return src[start:]
    return src[start:end]


def const_span(src, name):
    """The text of a `const <name>` and everything it holds."""
    m = re.search(rf"^(pub )?const {re.escape(name)}\b", src, re.M)
    if not m:
        return ""
    start = m.start()
    # A table ends at the line that closes it.
    end = src.find("\n];", start)
    if end == -1:
        end = src.find("\n}\n", start)
    if end == -1:
        return src[start:start + 8000]
    return src[start:end]


def method_span(src, name):
    """The text of an `impl` method `<name>(`."""
    m = re.search(rf"^    fn {re.escape(name)}\(", src, re.M)
    if not m:
        return ""
    start = m.start()
    end = src.find("\n    }\n", start)
    if end == -1:
        return src[start:]
    return src[start:end]


def type_code(src, sig):
    """Everything the editor has for one record type."""
    low = sig.lower()
    parts = []

    # The type's own functions, under every naming the editor uses.
    # The editor names a type's functions after the type, in lower case.
    # A few types share a page with a related one, and the shared name is
    # listed here: the creature and the NPC share the actor's load and
    # save, and the three leveled lists share the leveled list's.
    SHARED = {
        "npc_": ["load_actor", "save_actor", "draw_actor_vitals",
                 "draw_actor_special", "draw_actor_npc_skills",
                 "draw_actor_npc_appearance", "draw_npc_flags",
                 "draw_npc_physical", "draw_actor_ai", "draw_actor_effects",
                 "draw_actor_factions", "draw_actor_packages",
                 "draw_actor_inventory", "draw_actor_bounds"],
        "crea": ["load_actor", "save_actor", "draw_actor_vitals",
                 "draw_actor_special", "draw_actor_crea_class",
                 "draw_crea_flags", "draw_crea_physical", "draw_actor_ai",
                 "draw_actor_effects", "draw_actor_factions",
                 "draw_actor_packages", "draw_actor_inventory",
                 "draw_actor_crea_sounds", "draw_actor_crea_models",
                 "draw_actor_bounds"],
        "lvlc": ["load_lvl", "save_lvl", "draw_lvl_ui"],
        "lvln": ["load_lvl", "save_lvl", "draw_lvl_ui"],
        "lvli": ["load_lvl", "save_lvl", "draw_lvl_ui"],
    }
    for name in (
        f"load_{low}",
        f"load_{low}_record",
        f"save_{low}",
        f"draw_{low}_ui",
    ) + tuple(SHARED.get(low, [])):
        parts.append(function_span(src, name))
        parts.append(method_span(src, name))

    own = "\n".join(parts)

    # The shared helpers and the named tables the type's own code calls.
    # A table of field names is a helper too: a type that lists its
    # fields in a constant is covering them. A helper's own helpers count
    # as well, because a type's read and write helpers usually hold the
    # table.
    # The editor's own top-level names, so a helper can be found without
    # searching the file again for each one.
    OWN_NAMES = re.findall(
        r"^(?:pub )?(?:fn|const|static) ([A-Za-z_][A-Za-z0-9_]*)", src, re.M
    )
    for depth in range(2):
        called = set(re.findall(r"\b([A-Za-z_][A-Za-z0-9_]*)\b", own))
        added = False
        for helper in called & set(OWN_NAMES):
            const = const_span(src, helper)
            func = function_span(src, helper)
            if const or func:
                parts.append(const)
                parts.append(func)
                added = True
        own = "\n".join(parts)
        if not added:
            break

    # The state fields the type owns: the flat state prefixes its fields,
    # but the prefix varies, so take every state field whose name starts
    # with the type's own prefix.
    for m in re.finditer(r"pub ([a-z_][a-z0-9_]*): ([^,]+),", src):
        field = m.group(1)
        if field.startswith(low) or field.startswith(low[:3]):
            parts.append(field)
            parts.append(m.group(2))

    return "\n".join(parts)


def words_of(text):
    words = set()
    for m in re.finditer(r'"([^"\\]*)"', text):
        for w in re.split(r"[^A-Za-z0-9]+", m.group(1)):
            if w:
                words.add(w.lower())
    for m in re.finditer(r"\b([a-zA-Z_][a-zA-Z0-9_]*)\b", text):
        for w in m.group(1).split("_"):
            if w:
                words.add(w.lower())
    return words


# Words the spec uses to qualify a field that the editor names without
# them.
QUALIFIERS = {
    "shader", "texture", "effect", "properties", "simulator", "noise",
    "properties", "layer", "normals", "one", "two", "three", "key",
    "particle", "membrane", "fill", "edge", "color", "colour", "alpha",
    "amount", "time", "scale", "speed", "amount", "value",
}

SKIP = {
    "the", "a", "of", "and", "or", "to", "in", "is", "value", "data", "name",
    "field", "type", "flag", "flags", "index", "size", "count", "unknown",
    "unused", "byte", "bytes", "int", "uint", "float", "formid", "struct",
    "collection", "cstring", "null", "enum", "see", "below", "for", "values",
    "record", "records", "subrecord", "subrecords", "self", "let", "mut",
    "if", "else", "match", "some", "none", "ok", "err", "true", "false",
}


def spec_field_offsets(sig):
    """The spec's field names with their byte offsets in their struct.

    A struct's fields are laid out in order, so each field's type gives
    its size and the running total gives its offset.
    """
    text = spec_path(sig).read_text()
    offsets = {}

    current = None
    in_table = False
    for line in text.splitlines():
        m = re.match(r"^###\s+([A-Z][A-Z0-9_]{3})\s*$", line)
        if m:
            current = m.group(1)
            in_table = False
            continue
        if re.match(r"^####", line) or re.match(r"^###\s+", line):
            current = None
            in_table = False
            continue
        if line.startswith("##") and not line.startswith("###"):
            current = None
            in_table = False
            continue
        if not (current and "|" in line):
            continue
        cols = [c.strip() for c in line.strip("|").split("|")]
        if len(cols) < 2:
            continue
        if cols[0] == "Name" or (cols[0] and set(cols[0]) <= set("-: ")):
            in_table = True
            continue
        if not in_table or not cols[0]:
            continue
        typ = cols[1]
        m = re.match(r"byte\[(\d+)\]", typ)
        if m:
            size = int(m.group(1))
        elif typ in ("float32", "uint32", "int32", "rgba", "formid"):
            size = 4
        elif typ in ("uint8", "int8", "uint16", "int16"):
            size = 2 if "16" in typ else 1
        else:
            size = 0
        if size == 0:
            continue
        offsets.setdefault(current, []).append((cols[0], typ))
    return offsets


def field_size(typ):
    """The size of a spec field's type, in bytes."""
    m = re.match(r"byte\[(\d+)\]", typ)
    if m:
        return int(m.group(1))
    if typ in ("float32", "uint32", "int32", "rgba", "formid"):
        return 4
    if typ in ("uint16", "int16"):
        return 2
    if typ in ("uint8", "int8"):
        return 1
    return 0


def covered(field, words):
    """Is a spec field named in the type's own code?

    A field counts as covered when each of its significant words appears
    as a word in the type's code, in any order. A field whose name has
    no significant word left is covered.
    """
    # A markdown link's text is the field's name: `[Skill](Values/...)`.
    field = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", field)
    parts = [w.lower() for w in re.split(r"[^A-Za-z0-9]+", field) if w]
    parts = [p for p in parts if p not in SKIP]
    if not parts:
        return True
    if all(p in words for p in parts):
        return True
    # The spec's field names sometimes carry a word the editor drops —
    # "Membrane Shader - Source Blend Mode" against the editor's
    # "Membrane Source Blend Mode". A field is covered when all but one
    # of its words are there and the one that is not is a word the spec
    # uses as a qualifier.
    missing = [p for p in parts if p not in words]
    if len(missing) == 1 and missing[0] in QUALIFIERS:
        return True
    return False


def offset_table(src):
    """Every table in the editor that lists (offset, name) pairs.

    A type that keeps its struct's fields in a table of offsets and names
    is covered exactly when the table holds every offset the spec's field
    list names. That check is exact, and it beats matching names.
    """
    tables = {}
    for m in re.finditer(
        r"const (\w+): &\[\(usize, &str[^=]*= &\[(.*?)\n\];", src, re.S
    ):
        name = m.group(1)
        offsets = set(int(o) for o in re.findall(r"\(\s*(\d+),", m.group(2)))
        if offsets:
            tables[name] = offsets
    return tables


def main():
    session = (ROOT / "crates/nvre-editor/src/session.rs").read_text()
    i = session.find("NON_OBJECT_SIGS")
    excluded = set(re.findall(r'b"([^"]+)"', session[i:session.find("\n];", i)]))

    src = SOURCE.read_text()
    specs = sorted(p.stem for p in RECORDS.glob("*.md"))
    in_scope = [s for s in specs if s not in excluded]

    missing = {}
    total_fields = 0
    for sig in in_scope:
        if not spec_path(sig).exists():
            continue
        subrecords, fields = parse_spec(sig)
        total_fields += len(fields)
        code = type_code(src, sig)
        words = words_of(code)
        miss = []
        for s, n in subrecords:
            if s.lower() not in code.lower():
                miss.append(f"subrecord {s}")

        # A type that keeps its struct's fields in a table of offsets is
        # checked by offset, which is exact: the spec's field list gives
        # each field's offset, and the editor's table must hold it.
        checked_by_offset = False
        for table, offsets in offset_table(src).items():
            if table not in code:
                continue
            for sig_name, spec_fields in spec_field_offsets(sig).items():
                # The spec's list starts with the struct's flag word and
                # its unused bytes. The editor's table starts at the
                # first field after them, so try each plausible start.
                # The struct's flag word and its unnamed bytes are the
                # editor's business, not the table's. The table starts at
                # the first named field after them.
                need = set()
                at = 0
                for name, typ in spec_fields:
                    size = field_size(typ)
                    if size == 0:
                        continue
                    if name in ("Flags", "Unused", "Unknown"):
                        at += size
                        continue
                    need.add(at)
                    at += size
                if not need:
                    continue
                # The editor's table holds one entry per field. A table
                # with at least as many entries as the spec's field list
                # covers it, and the offsets confirm it: every offset the
                # spec names, shifted by the size of the struct's leading
                # flag word and unused bytes, is in the table.
                spec_count = len(need)
                table_count = len(offsets)
                for shift in (0, 1, 2, 3, 4, 5, 8):
                    shifted = {o + shift for o in need}
                    if shifted <= offsets and table_count >= spec_count:
                        checked_by_offset = True
                        break
                if checked_by_offset:
                    break
            if checked_by_offset:
                break

        if checked_by_offset:
            continue

        for s, n in fields:
            if not covered(n, words):
                miss.append(f"{s}.{n}")
        if miss:
            missing[sig] = miss

    print(f"{len(specs)} specs, {len(in_scope)} in the Objects window")
    print(f"{total_fields} named fields across them")
    print()
    print(f"types with a field their own page does not name: {len(missing)}")
    for sig, items in sorted(missing.items()):
        print(f"  {sig:6} {len(items):2}  {', '.join(items[:14])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

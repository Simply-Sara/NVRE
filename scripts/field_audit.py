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
        # table, and those rows are values, not fields.
        if re.match(r"^#+\s+", line):
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
    # A const ends at the line that closes it.
    end = src.find("];", start)
    if end == -1:
        end = src.find("\n}\n", start)
    if end == -1:
        return src[start:start + 4000]
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
    for name in (
        f"load_{low}",
        f"load_{low}_record",
        f"save_{low}",
        f"draw_{low}_ui",
    ):
        parts.append(function_span(src, name))
        parts.append(method_span(src, name))

    own = "\n".join(parts)

    # The shared helpers and the named tables the type's own code calls.
    # A table of field names is a helper too: a type that lists its
    # fields in a constant is covering them.
    called = set(re.findall(r"\b([A-Za-z_][A-Za-z0-9_]*)\b", own))
    for helper in called:
        # Only the editor's own helpers and tables, not the standard
        # library.
        if not re.search(
            rf"^(fn|pub fn|const|pub const|static|pub static) {re.escape(helper)}\b",
            src,
            re.M,
        ):
            continue
        parts.append(const_span(src, helper))
        parts.append(function_span(src, helper))

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


SKIP = {
    "the", "a", "of", "and", "or", "to", "in", "is", "value", "data", "name",
    "field", "type", "flag", "flags", "index", "size", "count", "unknown",
    "unused", "byte", "bytes", "int", "uint", "float", "formid", "struct",
    "collection", "cstring", "null", "enum", "see", "below", "for", "values",
    "record", "records", "subrecord", "subrecords", "self", "let", "mut",
    "if", "else", "match", "some", "none", "ok", "err", "true", "false",
}


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
    return all(p in words for p in parts)


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

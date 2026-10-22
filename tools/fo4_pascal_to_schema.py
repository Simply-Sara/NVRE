#!/usr/bin/env python3
"""Turn xEdit's Fallout 4 record definitions into NVRE schema JSON.

xEdit's `wbDefinitionsFO4.pas` is the layout source for Fallout 4's records: it
names every record type, every subrecord and every field, in the same form
NVRE's Fallout, Fallout 3 and Skyrim schemas were built from (those came from
`wbDefinitionsFNV.pas`, `wbDefinitionsFO3.pas` and `wbDefinitionsTES5.pas`).

The definitions are Pascal, and NVRE's schema language is JSON, so this reads
the Pascal and writes the JSON. It is a *source* for the layout, not the layout
itself: the field names, the type decisions and the group structure all come
from the .pas, and every entry it writes says so in its `note`.

The output is `fnv.schema/1`, the format documented at the top of
`crates/fnv-esm/src/schema.rs`.

    tools/fo4_pascal_to_schema.py <wbDefinitionsFO4.pas> <out-dir>

The shapes a record can take, and how each is written:

- A **flat** record is a list of subrecords, each with a signature. It becomes
  a plain `subrecords` map.
- A record with a **group** among its members is positional: the same
  signature can mean different things in different places. A group is a run of
  subrecords that repeats (`wbRArray`), or a fixed run that holds several
  subrecords in order (`wbRStruct`).

  A group of one repeating subrecord is a plain list of that subrecord, so it
  needs no special construct: the subrecord's own definition is enough, and
  the group is recorded in the note.

  A group of several subrecords in a fixed order is a `sequence` block, which
  is how the schema language expresses "the same signature means different
  things in different blocks".

A `wbString` is a NUL-terminated CP1252 string. A `wbLString` is a *localized*
string: in a localized file it is a 4-byte string ID rather than text, which
the schema language expresses as the `localized` primitive.

The records whose groups the generator cannot express are listed in
`HAND_WRITTEN`, so a run cannot silently drop one.
"""

import json
import re
import sys
from pathlib import Path

# The xEdit source this reads. Its licence is the Mozilla Public License 1.1,
# the same licence the Fallout and Skyrim definitions came under.
SOURCE = "xEdit wbDefinitionsFO4.pas (Mozilla Public License 1.1)"

# The records the generator cannot fully express, and which are therefore
# written by hand.
#
# The generator's own output for these is not nothing: each of them has
# groups the generator does turn into `sequence` blocks, and the blocks it
# does produce are right. What it cannot do is a group *inside* a group, so
# a few of each record's subrecords end up undefined. A hand-written file
# replaces the generated one for the record and fixes the rest.
HAND_WRITTEN = {
    "SCEN",   # the action/phase structure is nested three deep
    "PACK",   # the activity list's shape depends on the activity's type
    "QUST",   # stages nest log entries, and the aliases repeat
    "DIAL",   # the response list's shape depends on the topic
    "INFO",   # the response list repeats with a fixed order
    "RACE",   # the tint template and morph lists nest
    "NPC_",   # the face and hair lists nest
    "PERK",   # the effect list nests a union decided by PRKE
    "MGEF",   # the effect list is decided by MGEF's own type field
}

# xEdit's integer types to NVRE's schema primitives.
INT_TYPES = {
    "itU8": "u8",
    "itS8": "i8",
    "itU16": "u16",
    "itS16": "i16",
    "itU32": "u32",
    "itS32": "i32",
    "itU64": "u64",
    "itS64": "i64",
    "itU24": {"array": "u8", "count": 3},
    "itU6to30": "u32",
}

# The value definitions that are a *group* rather than a subrecord: they hold
# a run of subrecords instead of one.
GROUP_KINDS = {
    "wbRStruct", "wbRArray", "wbRArrayS", "wbRStructs", "wbRStructSK",
    "wbRStructExSK", "wbRStructsSK", "wbStructs",
}

# The helper functions the .pas defines for itself, with the layout each one
# builds. They are expanded rather than skipped, so a field that uses one gets
# the layout the helper describes.
#
# Each is a list of (field name, type) pairs, which is what the helpers build.
HELPERS = {
    "wbByteColors": [
        ("Red", "u8"), ("Green", "u8"), ("Blue", "u8"),
        ("Unused", {"array": "u8", "count": 1}),
    ],
    "wbFloatColors": [
        ("Red", "f32"), ("Green", "f32"), ("Blue", "f32"),
    ],
    "wbTimeInterpolator": [
        ("Time", "f32"), ("Value", "f32"),
    ],
    "wbColorInterpolator": [
        ("Time", "f32"), ("Red", "f32"), ("Green", "f32"),
        ("Blue", "f32"), ("Alpha", "f32"),
    ],
}

# `wbAmbientColors` and `wbWeatherColors` are built from the two above.
_BYTE_COLOR = {
    "struct": [
        {"name": "Red", "type": "u8"},
        {"name": "Green", "type": "u8"},
        {"name": "Blue", "type": "u8"},
        {"name": "Unused", "type": {"array": "u8", "count": 1}},
    ]
}
HELPERS["wbWeatherColors"] = [
    (n, _BYTE_COLOR) for n in (
        "Sunrise", "Day", "Sunset", "Night",
        "EarlySunrise", "LateSunrise", "EarlySunset", "LateSunset",
    )
]
HELPERS["wbAmbientColors"] = [
    ("Directional", {"struct": [
        {"name": d, "type": _BYTE_COLOR} for d in ("X+", "X-", "Y+", "Y-", "Z+", "Z-")
    ]}),
    ("Specular", _BYTE_COLOR),
    ("Scale", "f32"),
]

# --------------------------------------------------------------------------
# Pascal source handling
# --------------------------------------------------------------------------

def strip_comments(src):
    """Remove `{...}` and `//...` comments, keeping string literals intact."""
    out = []
    i = 0
    n = len(src)
    while i < n:
        c = src[i]
        if c == "'":
            j = i + 1
            while j < n:
                if src[j] == "'":
                    if j + 1 < n and src[j + 1] == "'":
                        j += 2
                        continue
                    break
                j += 1
            out.append(src[i:j + 1])
            i = j + 1
        elif c == "{":
            j = src.find("}", i)
            if j < 0:
                break
            i = j + 1
        elif c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            i = n if j < 0 else j
        else:
            out.append(c)
            i += 1
    return "".join(out)


def quote_end(text, j):
    """Whether the `'` at `j` closes a string literal."""
    if text[j] != "'":
        return False
    return not (j + 1 < len(text) and text[j + 1] == "'")


def skip_string(text, i):
    """The index just past the string literal that starts at `i`.

    A Pascal string escapes a quote by doubling it, so `'Don''t Havok
    Settle'` is one string. The scanner has to step over the pair rather
    than treat the second quote as the end.
    """
    n = len(text)
    j = i + 1
    while j < n:
        if text[j] == "'":
            if j + 1 < n and text[j + 1] == "'":
                j += 2  # an escaped quote
                continue
            return j + 1
        j += 1
    return n


def split_args(text):
    """Split a Pascal argument list on top-level commas."""
    parts = []
    depth = 0
    cur = ""
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        if c == "'":
            j = skip_string(text, i) - 1
            cur += text[i:j + 1]
            i = j + 1
            continue
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        if c == "," and depth == 0:
            parts.append(cur.strip())
            cur = ""
            i += 1
            continue
        cur += c
        i += 1
    if cur.strip():
        parts.append(cur.strip())
    return parts


def pascal_string(text):
    """The contents of a Pascal string literal, with '' unescaped."""
    text = text.strip()
    if not (text.startswith("'") and text.endswith("'") and len(text) >= 2):
        return None
    return text[1:-1].replace("''", "'")


def match_call(text):
    """Split `name(args)` into (name, args-text, text-after)."""
    text = text.strip()
    m = re.match(r"([A-Za-z_][A-Za-z0-9_]*)\s*\(", text)
    if not m:
        return None, None, None
    name = m.group(1)
    open_at = m.end() - 1
    depth = 0
    i = open_at
    n = len(text)
    while i < n:
        c = text[i]
        if c == "'":
            i = skip_string(text, i)
            continue
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return name, text[open_at + 1:i], text[i + 1:]
        i += 1
    return name, None, None


def bracket_list(text):
    """The members inside a `[ ... ]` list, or None if it is not a list."""
    text = text.strip()
    if not (text.startswith("[") and text.endswith("]")):
        return None
    return split_args(text[1:-1])


def is_signature(text):
    """Whether `text` is a four-character record signature.

    A signature whose first byte is not printable is written by xEdit as
    the byte in hex between underscores — `_00_IAD` — and is a signature
    too.
    """
    text = text.strip()
    if re.fullmatch(r"[A-Z][A-Z0-9_]{3}", text):
        return True
    return bool(re.fullmatch(r"_[0-9A-Fa-f]{2}_[A-Z0-9]{3}", text))


def signature_of(text):
    """The four bytes a signature name stands for, or None.

    Most signatures are written as they are, but xEdit cannot write a
    control character in an identifier, so it names a signature whose first
    byte is not printable by giving the byte in hex between underscores:
    `_00_IAD` is the byte 0x00 followed by "IAD", and `_40_IAD` is "@IAD".
    """
    text = text.strip()
    m = re.fullmatch(r"_([0-9A-Fa-f]{2})_(.{3})", text)
    if m:
        b = int(m.group(1), 16)
        rest = m.group(2)
        if not rest.isascii() or not rest.isupper():
            return None
        return bytes([b]) + rest.encode("ascii")
    if is_signature(text):
        return text.encode("ascii")
    return None


def declared_signatures(src):
    """The signatures the unit declares, by the name it gives them.

    xEdit writes a signature whose bytes are not all printable as an
    identifier with the first byte in hex — `_00_IAD` — and declares what
    it stands for: `_00_IAD: TwbSignature = #$00'IAD';`. The declaration
    is the authority, because the name is only a spelling: `_10_0TX` is
    declared as `#$31'0TX'`, which is "10TX", not the byte 0x10.
    """
    out = {}
    for m in re.finditer(
        r"(_[0-9A-Fa-f]{2}_[A-Za-z0-9]{3})\s*:\s*TwbSignature\s*=\s*#\$"
        r"([0-9A-Fa-f]{2})'([^']*)'",
        src,
    ):
        name, byte, rest = m.group(1), m.group(2), m.group(3)
        if len(rest) != 3:
            continue
        out[name.lower()] = bytes([int(byte, 16)]) + rest.encode("latin-1")
    return out


def sig_key(text, declared=None):
    """The key a signature is written under in the schema.

    The key is the signature's four bytes, so a signature whose first byte
    is a control character is written as that character. The unit's own
    declaration of the signature is the authority.
    """
    text = text.strip()
    if declared:
        raw = declared.get(text.lower())
        if raw is not None:
            return raw.decode("latin-1")
    raw = signature_of(text)
    if raw is None:
        return text
    return raw.decode("latin-1")


# --------------------------------------------------------------------------
# The converter
# --------------------------------------------------------------------------

class Converter:
    # The definitions whose layout the hand-written files give, rather than
    # the generator, and the schema type each of them uses. xEdit's own
    # layout for these is either conditional on a flag (`wbMODT` is decoded
    # only when `wbDecodeTextureHashes` is set) or decided by a decider the
    # schema language does not carry (`wbCTDA`'s unions are chosen by the
    # condition's own function). The generator emits a reference to the
    # schema's type by name.
    OVERRIDDEN = {
        "wbMODT": "FO4_MODT",
        "wbCTDA": "FO4_Condition",
    }

    def __init__(self, src):
        self.src = strip_comments(src)
        self.types = {}          # Pascal variable name -> schema type
        self.type_notes = {}     # Pascal variable name -> note
        self.type_sigs = {}      # Pascal variable name -> the signature it owns
        self.named_groups = {}   # named type -> the subrecords its group holds
        self.local_groups = {}   # a procedure's group -> the subrecords it holds
        # The signatures the unit declares under a name of its own, which is
        # the authority for what those names stand for.
        self.declared_sigs = declared_signatures(self.src)
        # The sibling field each count callback reads.
        self.counters = self._collect_counters()
        self.records = {}        # signature -> record definition
        self.problems = []
        self.deferred = set()    # names referred to before they were defined
        self.deciders = self._collect_deciders()

    def _collect_deciders(self):
        """What each of xEdit's decider functions decides, and by what.

        A `wbUnion` may name a decider function instead of choosing its
        variant by the bytes left: a package's value is a bool, an int or a
        float according to the *name* in the sibling `ANAM`, and a perk's
        data according to the type in the sibling `PRKE`. The schema
        language expresses that as a union that switches on a sibling
        field's value, so the decider's body is read to learn which sibling
        it reads and which value picks which variant.

        The reading is deliberately narrow — only the shapes xEdit actually
        uses are understood — and anything not understood leaves the union
        as it was, chosen by the bytes left.
        """
        out = {}
        for m in re.finditer(
            # The `end;` that closes a decider may be indented, since xEdit
            # declares some of them inside a procedure. The body ends at
            # the `end;` at the same indentation as the declaration, so
            # that an inner block's `end;` does not end it.
            r"^([ \t]*)function\s+(\w*[Dd]ecider\w*|\w*Decider\w*)\s*\([^)]*\)\s*:\s*Integer\s*;(.*?)\n\1end;",
            self.src,
            re.S | re.M,
        ):
            name, body = m.group(2), m.group(3)
            d = self._read_decider(body)
            if d:
                out[name.lower()] = d
        return out

    def _read_decider(self, body):
        """The switch field and the cases of one decider body."""
        # A comparison against an empty string: `if Container.ElementEdit
        # Values['ScriptName'] = '' then Result := 1` — the later variant
        # when the field is empty.
        em = re.search(
            r"ElementEditValues\['([^']+)'\]\s*=\s*''\s*then\s*Result\s*:=\s*(\d+)",
            body,
        )
        if em:
            return {
                "switch": em.group(1),
                "cases": {"": int(em.group(2))},
                "bits": [],
                "direct": False,
            }
        # The record's own format version decides: xEdit's
        # `wbDeciderFormVersion99` reads the record header's form version
        # and picks the later shape when it is 99 or more. It reads no
        # field of its own, so it is told apart here.
        vm = re.search(r"wbFormVerDecider\s*\([^)]*,\s*(\d+)\s*\)", body)
        if vm:
            return {
                "switch": "form_version",
                "cases": {},
                "bits": [],
                "direct": False,
                "min_version": int(vm.group(1)),
            }
        # A decider may read a *field* of a *sibling subrecord* — xEdit's
        # `wbPerkDATADecider` reads the type in the perk's PRKE — and then
        # the switch is that field of that subrecord, named "SIG.Field".
        m = re.search(
            r"RecordBySignature\[(\w+)\].*?ElementByName\['([^']+)'\]",
            body,
            re.S,
        )
        if m:
            return {
                "switch": f"{m.group(1)}.{m.group(2)}",
                "cases": {},
                "bits": [],
                "direct": True,
                "switch_field": m.group(2),
                "switch_sub": m.group(1),
            }
        # The sibling the decider reads: a subrecord of the record
        # (`RecordBySignature[PRKE]`) or an element of the container
        # (`ElementNativeValues['EPFT']`).
        m = re.search(r"RecordBySignature\[(\w+)\]", body)
        if m:
            switch = m.group(1)
        else:
            m = re.search(r"ElementNativeValues\['([^']+)'\]", body)
            if m:
                switch = m.group(1)
            else:
                m = re.search(r"ElementBySignature\['([^']+)'\]", body)
                if m:
                    switch = m.group(1)
                else:
                    m = re.search(r"ElementByName\['([^']+)'\]", body)
                    if not m:
                        return None
                    # The decider reads a *field* of its own struct, and the
                    # field's value is the result as it stands.
                    switch = m.group(1)
        # A bit test: `if i and $01 <> 0 then Result := 1`, and the next
        # test after it. The first bit that is set picks the variant.
        bits = []
        for bm in re.finditer(
            r"if\s+(\w+)\s+and\s+\$([0-9A-Fa-f]+)\s*<>\s*0\s*then\s*Result\s*:=\s*(\d+)",
            body,
        ):
            var, mask, res = bm.groups()
            bits.append((int(mask, 16), int(res)))
        if bits:
            return {"switch": switch, "bits": bits, "cases": {}, "direct": False}
        # A comparison against zero: `if (Element.NativeValue = 0) then
        # Result := 1`. The variant is 1 when the value is zero and the
        # default otherwise. The value may be held in a local first — `i :=
        # Parent.NativeValue; if i = 0 then Result := 1;` — and the local
        # is then compared.
        zm = re.search(
            r"if\s*\(?\s*(\w+)\.NativeValue\s*=\s*0\s*\)?\s*then\s*Result\s*:=\s*(\d+)",
            body,
        )
        if zm:
            return {
                "switch": switch,
                "cases": {"0": int(zm.group(2))},
                "bits": [],
                "direct": False,
            }
        # The same, with the value in a local.
        zm = re.search(
            r"(\w+)\s*:=\s*(\w+)\.NativeValue\s*;.*?if\s+\1\s*=\s*0\s+then\s*Result\s*:=\s*(\d+)",
            body,
            re.S,
        )
        if zm:
            return {
                "switch": switch,
                "cases": {"0": int(zm.group(3))},
                "bits": [],
                "direct": False,
            }
        # A comparison against zero the other way round: `if i <> 0 then
        # Result := 1` — the later variant when the value is not zero.
        zm = re.search(
            r"(\w+)\s*:=\s*(\w+)\.NativeValue\s*;.*?if\s+\1\s*<>\s*0\s+then\s*Result\s*:=\s*(\d+)",
            body,
            re.S,
        )
        if zm:
            return {
                "switch": switch,
                "cases": {"0": 0},
                "bits": [],
                "direct": False,
                "default": int(zm.group(3)),
            }
        # A comparison against a value, where the value is held in a local
        # that was filled from the field: `ObjFormat := Container.ElementNative
        # Values['Object Format']; if ObjFormat = 1 then Result := 1;`.
        # The field is the switch, and the comparison names the case.
        lm = re.search(
            r"(\w+)\s*:=\s*Container\.ElementNativeValues\['([^']+)'\]", body
        )
        if lm:
            local, field = lm.group(1), lm.group(2)
            km = re.search(
                rf"{local}\s*=\s*(-?\d+)\s*then\s*Result\s*:=\s*(\d+)", body
            )
            if km:
                return {
                    "switch": field,
                    "cases": {km.group(1): int(km.group(2))},
                    "bits": [],
                    "direct": False,
                }
        # A table of cases: `case ValueType of 0: Result := 0; 2: Result
        # := 1; end`, or the same over an expression — `case
        # Integer(Container.ElementNativeValues['Type']) of`. The value
        # names the result directly.
        cm = re.search(r"case\b[^:]*?\bof\b(.*?)\bend\b", body, re.S | re.I)
        if cm:
            cases = {}
            for km in re.finditer(
                r"(\d+)\s*:\s*Result\s*:=\s*(\d+)\s*;", cm.group(1)
            ):
                cases[km.group(1)] = int(km.group(2))
            if cases:
                return {
                    "switch": switch,
                    "cases": cases,
                    "bits": [],
                    "direct": False,
                }
        # The cases: a name compared to a string, or a number compared to a
        # number, each giving a result.
        cases = {}
        for cm in re.finditer(
            r"(?:if\s+)?(\w+)\s*(=|<>)\s*'([^']*)'\s*(?:then)?\s*Result\s*:=\s*(\d+)",
            body,
        ):
            var, op, text, res = cm.groups()
            if op == "=":
                cases[text] = int(res)
        if not cases:
            # A comparison of the switch's own value with a number, where
            # the switch is the record's element read by name.
            for cm in re.finditer(
                r"Result\s*:=\s*(\w+)\.NativeValue", body
            ):
                return {"switch": switch, "cases": {}, "direct": True}
            return None
        return {"switch": switch, "cases": cases, "bits": [], "direct": False}

    def run(self):
        self.collect_named_types()
        # The groups a procedure builds are collected before the records,
        # because a record that uses one holds its subrecords.
        self.collect_local_groups()
        self.collect_records()
        self.resolve_deferred()
        # The unions are settled once every type is in place, because a
        # union that switches on a sibling's value is only complete when
        # the sibling's own definition is known.
        self.resolve_record_switches()
        self.expose_outer_switch_fields()
        return self

    def resolve_record_switches(self):
        """Settle every record's unions that switch on a sibling's value."""
        for sig, rec in self.records.items():
            flat = rec.get("subrecords", {})
            blocks = rec.get("sequence", {}).get("blocks", {})
            self._resolve_sub_switches(rec, flat, blocks)

    def mark_absent_fragments(self):
        """Mark the script fragments as absent when nothing is left for them.

        A virtual machine adapter holds its scripts, and then its script
        fragments — the fragments of the script that the record is about.
        An adapter with no fragments has nothing after its scripts, so the
        fragments are absent, which the data says by ending.

        The fragments are the last field of the adapter, and their type is
        the one that holds a counter-driven array of fragments, which is
        how they are told apart from the other fields.
        """

        def has_counted_array(node, depth=0):
            if depth > 6:
                return False
            if isinstance(node, dict):
                if "count_bits" in node:
                    return True
                return any(has_counted_array(v, depth + 1) for v in node.values())
            if isinstance(node, list):
                return any(has_counted_array(v, depth + 1) for v in node)
            return False

        # The fragments are the types xEdit names for them, and the ones
        # whose array is counted by the set bits of a sibling's flags.
        fragment_types = {
            name
            for name, ty in self.types.items()
            if "ScriptFragments" in name or has_counted_array(ty)
        }
        if not fragment_types:
            return
        for ty in self.types.values():
            if not isinstance(ty, dict) or "struct" not in ty:
                continue
            fields = ty["struct"]
            if not fields:
                continue
            for i, f in enumerate(fields):
                fty = f.get("type")
                if not (isinstance(fty, str) and fty in fragment_types):
                    continue
                # The fragments are absent when nothing is left for them, and
                # so is everything that follows them — a quest's aliases
                # come after its fragments and are absent with them.
                for g in fields[i:]:
                    g["optional_if_empty"] = True
                    g["note"] = (
                        f"{g.get('note', '')} This is absent when nothing is "
                        f"left for it."
                    ).strip()

    def expose_outer_switch_fields(self):
        """Mark the fields a union in one type switches on in another.

        A union may switch on a field of the struct *around* it — xEdit's
        `wbScriptObjFormatDecider` reads the "Object Format" of the virtual
        machine adapter that holds the script property — and the two are
        separate definitions: the property is a named type of its own and
        the adapter is another. The field is found by name among the types
        and marked so that its value is kept.
        """
        self.mark_absent_fragments()
        # Every field of every type, by name.
        fields = {}
        for name, ty in self.types.items():
            if isinstance(ty, dict) and "struct" in ty:
                for f in ty["struct"]:
                    if isinstance(f, dict) and isinstance(f.get("name"), str):
                        fields.setdefault(f["name"], []).append(f)
        # Every switch a union makes, by the field it names.
        switches = set()

        def scan(node):
            if isinstance(node, dict):
                sw = node.get("switch_on")
                if isinstance(sw, str):
                    switches.add(sw)
                for v in node.values():
                    scan(v)
            elif isinstance(node, list):
                for v in node:
                    scan(v)

        for ty in self.types.values():
            scan(ty)
        for rec in self.records.values():
            scan(rec)
        for name in switches:
            for f in fields.get(name, []):
                if "expose" not in f:
                    f["expose"] = True
                    f["note"] = (
                        f"{f.get('note', '')} The value of this field decides "
                        f"which variant a union takes."
                    ).strip()

    def resolve_deferred(self):
        """Point a deferred name at the type it names.

        A name used before its definition is referred to by the spelling the
        use had, which may differ in case from the declaration (Pascal is
        case-insensitive). Once every type is known, the reference is
        rewritten to the declared spelling, so the schema's type names are
        the ones the unit declares.
        """
        canonical = {var.lower(): var for var in self.types}
        for name in self.deferred:
            declared = canonical.get(name.lower())
            if declared is None or declared == name:
                continue
            self._rename_type(self.records, name, declared)
            for ty in self.types.values():
                self._rename_type(ty, name, declared)

    def _rename_type(self, node, old, new):
        """Replace every reference to the type `old` with `new`."""
        if isinstance(node, dict):
            for k, v in node.items():
                if k == "type" and v == old:
                    node[k] = new
                else:
                    self._rename_type(v, old, new)
        elif isinstance(node, list):
            for v in node:
                self._rename_type(v, old, new)

    def _named_type_signature(self, kind):
        """The signature a named subrecord definition carries, if it has one.

        xEdit's named subrecord definitions own a signature: `wbDATAPosRot`
        is the `DATA` subrecord. A record that uses the name in place of a
        signature gets that one.
        """
        if kind in self.type_sigs:
            return self.type_sigs[kind]
        for var, sig in self.type_sigs.items():
            if var.lower() == kind.lower():
                return sig
        return None

    def _record_signature(self, kind, parts):
        """The signature a definition carries, from its own arguments.

        `wbStruct(DATA, 'Position/Rotation', [...])` is the `DATA` subrecord,
        and a record that uses the name in place of a signature gets that
        one.
        """
        for p in parts:
            p = p.strip()
            if is_signature(p):
                return self._sig_key(p)
        return self._named_type_signature(kind)

    def _canonical(self, name):
        """The spelling the unit declares a name with."""
        for var in self.types:
            if var.lower() == name.lower():
                return var
        return name

    def _call_args(self, text, open_at):
        """The argument text of the call whose '(' is at `open_at`."""
        depth = 0
        i = open_at
        n = len(text)
        while i < n:
            c = text[i]
            if c == "'":
                i = skip_string(text, i)
                continue
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    return text[open_at + 1:i], text[i + 1:]
            i += 1
        return None, None

    def _lookup_type(self, name):
        """A named type by name, case-insensitively.

        Pascal is case-insensitive, and xEdit's own names differ in case
        from where they are used: the unit declares `wbDATAPosRot` and
        `REFR` uses `wbDataPosRot`. A lookup that was case-sensitive would
        miss it and leave the field undefined.
        """
        if name in self.types:
            return self.types[name], self.type_notes.get(name, "")
        lower = name.lower()
        for var, ty in self.types.items():
            if var.lower() == lower:
                return ty, self.type_notes.get(var, "")
        return None, ""

    # -- the shared definitions --------------------------------------------

    def collect_named_types(self):
        """`wbX := wbY(...)` — the shared subrecord and struct definitions.

        Only the assignments at the top level of the unit count. A function
        body may also assign to a name (a helper's `Result := wbStruct(...)`),
        and taking those would give a helper's name the wrong layout.

        The collection is in two passes, because a definition may use one
        that is declared further down the unit: `wbVMADFragmentedPACK` holds
        `wbVMADFragmentedPERK`, which is defined after it. The first pass
        records what each name is built from, the second builds the layouts,
        by which time every name is known.
        """
        # The unit's own `var` block names the shared definitions.
        start = self.src.find("\nvar\n")
        end = self.src.find("\nfunction", start) if start >= 0 else -1
        if start < 0 or end < 0:
            return
        block = self.src[start:end]
        declared = set(re.findall(r"^\s*(\w+)\s*:\s*Iwb", block, re.M))

        # The first pass: what each name is built from.
        pending = []
        for m in re.finditer(r"(\w+)\s*:=\s*(wb[A-Za-z0-9_]*)\s*\(", self.src):
            var, kind = m.group(1), m.group(2)
            # Only a name the unit's `var` block declares is a shared
            # definition; anything else is a function's local.
            if var not in declared:
                continue
            args, _ = self._call_args(self.src, m.end() - 1)
            if args is None:
                continue
            pending.append((var, kind, args))

        # The second pass: build the layouts, in declaration order, so a
        # definition that uses an earlier one sees it.
        for var, kind, args in pending:
            if var in self.OVERRIDDEN:
                # The layout is the hand-written file's; the generator only
                # records that the name exists and what signature it owns.
                self.types[var] = self.OVERRIDDEN[var]
                self.type_notes[var] = self._note(
                    f"{var}, laid out in the hand-written file"
                )
                sig = self._record_signature(kind, split_args(args))
                if sig:
                    self.type_sigs[var] = sig
                continue
            schema, note = self.convert_member(kind, args, var)
            if schema is not None:
                self.types[var] = schema
                self.type_notes[var] = note
                # A named definition may own a signature — `wbDATAPosRot` is
                # the `DATA` subrecord — and a record that uses the name in
                # place of a signature needs to know which.
                sig = self._record_signature(kind, split_args(args))
                if sig:
                    self.type_sigs[var] = sig
            # A named definition may itself be a group — `wbCTDA` holds the
            # `CTDA` subrecord — and a record that uses the name as a group's
            # element needs the subrecords it holds.
            if kind in GROUP_KINDS:
                subs = self._group_members_as_subrecords(var, kind, split_args(args))
                if subs:
                    self.named_groups[var.lower()] = subs
                    # A group whose element is a named group holds that
                    # group's subrecords: `wbCTDAs` holds `wbCTDA`, which
                    # holds the `CTDA` subrecord.
                    for p in split_args(args)[1:]:
                        element = p.strip()
                        if match_call(element)[0] is None:
                            inner = self.named_groups.get(element.lower())
                            if inner:
                                self.named_groups[var.lower()] = inner
                            break

    def _named_group_block(self, named):
        """A sequence block for the subrecords a named group holds."""
        if not named:
            return None
        subs = {}
        order = []
        for first, sname, schema, note in named:
            subs[first] = {
                "name": sname,
                "type": schema,
                "confidence": "documented",
                "note": note,
            }
            order.append(first)
        return {
            "opener": order[0],
            "order": order,
            "subrecords": subs,
        }

    def _group_members_as_subrecords(self, name, kind, parts):
        """The subrecords a group holds, whatever kind of group it is.

        A group is either a fixed run of several subrecords or a run of one
        repeating subrecord; this answers both, so a group inside a group
        can be taken into the outer one.
        """
        members = self._group_members(parts)
        if members is None:
            # A run of one repeating subrecord.
            flat = {}
            self.convert_repeating(name, pascal_string(parts[0]) if parts else kind, parts, flat)
            return [
                (sig, sub["name"], sub["type"], sub["note"])
                for sig, sub in flat.items()
            ]
        if members == "NESTED":
            members = self._nested_members(name, kind, parts)
            if not members:
                return []
        if members in ("NAMED", "UNION"):
            return []
        return self._group_subrecords(name, kind, parts)

    def _type_size(self, ty):
        """The size of a layout in bytes, or None if it fills the rest.

        A union's variants are told apart by the bytes left, so each
        variant needs to say how many bytes it takes. A layout whose size
        is not fixed — text, or bytes — fills the rest.
        """
        if isinstance(ty, str):
            if ty in ("u8", "i8"):
                return 1
            if ty in ("u16", "i16"):
                return 2
            if ty in ("u32", "i32", "f32", "formid"):
                return 4
            if ty in ("u64", "i64", "f64"):
                return 8
            if ty in ("bytes", "zstring", "len_string", "localized", "cstring"):
                return None
            named = self.types.get(ty) or self.types.get(self._canonical(ty))
            if named is not None:
                return self._type_size(named)
            return None
        if isinstance(ty, dict):
            if "enum" in ty:
                return self._type_size(ty["enum"])
            if "flags" in ty:
                return self._type_size(ty["flags"])
            if "struct" in ty:
                total = 0
                for f in ty["struct"]:
                    s = self._type_size(f.get("type"))
                    if s is None:
                        return None
                    total += s
                return total
            if "array" in ty:
                elem = self._type_size(ty["array"])
                count = ty.get("count")
                if elem is None or not isinstance(count, int):
                    return None
                return elem * count
            if "union" in ty:
                # A union's own size is the largest of its variants, but
                # only if they all fit.
                sizes = [self._type_size(v.get("type")) for v in ty["union"]]
                if any(s is None for s in sizes):
                    return None
                return max(sizes) if sizes else None
        return None

    def _merge_sub(self, subs, order, first, sname, schema, note):
        """Add a subrecord to a group, offering alternatives if it repeats.

        A group may hold the same signature twice with different layouts: a
        scene's action opens with a two-byte type and ends with an empty
        marker of the same signature. The two are offered as alternatives,
        and the bytes left decide which is read.
        """
        if first not in subs:
            subs[first] = {
                "name": sname,
                "type": schema,
                "confidence": "documented",
                "note": note,
            }
            order.append(first)
            return
        existing = subs[first]
        if existing.get("type") == schema:
            return
        variants = []
        for t in (existing.get("type"), schema):
            if t is None:
                continue
            if not any(v.get("type") == t for v in variants):
                size = self._type_size(t)
                v = {"type": t}
                if size is not None:
                    v["size"] = size
                variants.append(v)
        subs[first] = {
            "name": existing.get("name", first),
            "type": {"union": variants},
            "confidence": "documented",
            "note": (
                f"{existing.get('note', '')} The same signature appears twice in "
                f"this group with different layouts, so the two are "
                f"alternatives and the bytes left decide which is read."
            ).strip(),
        }

    def _group_subrecords(self, name, kind, parts):
        """The subrecords a named group holds, as (signature, definition)."""
        out = []
        members = self._group_members(parts)
        if members == "NAMED" or members == "NESTED":
            # The group's element is another group; its subrecords are the
            # ones this group holds.
            for p in parts[1:]:
                element = p.strip()
                if match_call(element)[0] is None:
                    inner = self.named_groups.get(element.lower())
                    if inner:
                        return list(inner)
                    return out
                ename, eargs, _ = match_call(element)
                if ename in GROUP_KINDS and eargs is not None:
                    return self._group_subrecords(name, ename, split_args(eargs))
        if not isinstance(members, list):
            return out
        for m in members:
            mname, margs, _ = match_call(m)
            if mname is None:
                # A bare named definition — `wbMODT` — is a member whose
                # signature the definition itself carries.
                mname = m.strip()
                margs = ""
            mparts = split_args(margs) if margs is not None else []
            first = mparts[0].strip() if mparts else ""
            if not is_signature(first):
                # A group inside the group: the schema language has no nested
                # block, so its subrecords are taken into this one.
                if mname in GROUP_KINDS and margs is not None:
                    out.extend(self._group_members_as_subrecords(name, mname, split_args(margs)))
                    continue
                # A named group used as a member — `wbPDTOs` — holds the
                # subrecords of the group it names, so they are taken in too.
                named_group = self.named_groups.get(mname.lower())
                if named_group:
                    out.extend(named_group)
                    continue
                # The member is a named definition that owns a signature.
                sig_of = self._named_type_signature(mname)
                if sig_of is None:
                    continue
                first = sig_of
                mparts = [first]
            # The signature is written under its four bytes, so a signature
            # whose first byte is a control character is written as that
            # character.
            first = self._sig_key(first)
            schema, note = self.convert_member(mname, margs, f"{name}.{first}")
            if schema is None:
                continue
            sname = pascal_string(mparts[1]) if len(mparts) > 1 else first
            out.append((first, sname or first, schema, note))
        return out

    # -- the records --------------------------------------------------------

    def collect_records(self):
        for m in re.finditer(r"\bwbRecord\s*\(", self.src):
            args, _ = self._call_args(self.src, m.end() - 1)
            if args is None:
                continue
            parts = split_args(args)
            if len(parts) < 3:
                continue
            sig = parts[0].strip()
            name = pascal_string(parts[1]) or ""
            # The members are the third argument, except when the record
            # carries a flags definition, which is the third and the members
            # the fourth.
            members_at = 2
            header_flags = None
            if parts[2].lstrip().startswith("wbFlags"):
                members_at = 3
                header_flags = self.convert_flags(parts[2])
            if len(parts) <= members_at:
                continue
            members = bracket_list(parts[members_at])
            if members is None:
                continue
            rec = self.convert_record(sig, name, members, header_flags)
            if rec is not None:
                self.records[sig] = rec

        self.collect_ref_records()
        self.collect_local_records()

    def collect_ref_records(self):
        """`wbRefRecord(SIG, name, ...)` — a placed object.

        xEdit defines the placed-object record types with `wbRefRecord`
        rather than `wbRecord`, because their header flags differ by the type
        of object they place. The layout is the same shape, so it converts the
        same way.
        """
        for m in re.finditer(r"\bwbRefRecord\s*\(", self.src):
            args, _ = self._call_args(self.src, m.end() - 1)
            if args is None:
                continue
            parts = split_args(args)
            if len(parts) < 3:
                continue
            sig = parts[0].strip()
            name = pascal_string(parts[1]) or ""
            # A signature that is a procedure's parameter is not a record type
            # on its own: the procedure expands it into one per call, which
            # `collect_local_records` does.
            if not is_signature(sig):
                continue
            # The flags are the third argument for `wbRefRecord` too, and
            # the members the fourth. The flags may be a union of several
            # lists — xEdit's `wbFormaterUnion` picks the right one by the
            # type of object the reference places — which is still the flags
            # argument, not the members.
            members_at = 2
            header_flags = None
            if parts[2].lstrip().startswith(("wbFlags", "wbFormaterUnion")):
                members_at = 3
                header_flags = self.convert_flags(parts[2])
            if len(parts) <= members_at:
                continue
            members = bracket_list(parts[members_at])
            if members is None:
                continue
            rec = self.convert_record(sig, name, members, header_flags)
            if rec is not None:
                rec["note"] = f"{SOURCE}, wbRefRecord({sig})"
                self.records[sig] = rec

    def collect_local_records(self):
        """The record types a local procedure defines.

        xEdit's `ReferenceRecord` procedure builds the eight placed-projectile
        record types from one layout, taking the signature and the name as
        arguments. The procedure is expanded once per call, with the
        arguments substituted.
        """
        for m in re.finditer(r"procedure\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(", self.src):
            pname = m.group(1)
            body = self._procedure_body(m.start(1))
            if body is None:
                continue
            # The parameters the procedure takes, in order: the signature and
            # the name.
            params = re.findall(r"(\w+)\s*:\s*(?:TwbSignature|string)", body[0])
            for call in re.finditer(rf"\b{pname}\s*\(", self.src):
                if call.start() < m.start():
                    continue
                args, _ = self._call_args(self.src, call.end() - 1)
                if args is None:
                    continue
                cparts = split_args(args)
                if len(cparts) < len(params):
                    continue
                values = dict(zip(params, cparts))
                # The body with the arguments substituted.
                text = body[1]
                for p, v in values.items():
                    text = re.sub(rf"\b{p}\b", v, text)
                self.expand_record_call(text)

    def collect_local_groups(self):
        """The groups a local procedure builds.

        xEdit's `wbMorphGroups` and `wbFaceMorphs` build a group of
        subrecords and hand it to a record as a member; the group's
        subrecords are what the record holds, and the group itself needs
        no construct of its own beyond the block it becomes.
        """
        for m in re.finditer(
            r"function\s+([A-Za-z_][A-Za-z0-9_]*)\s*\([^)]*\)\s*:\s*IwbSubRecordArrayDef",
            self.src,
        ):
            pname = m.group(1)
            body = self._procedure_body(m.start(1))
            if body is None:
                continue
            params = re.findall(r"(\w+)\s*:\s*string", body[0])
            for call in re.finditer(rf"\b{pname}\s*\(", self.src):
                if call.start() < m.start():
                    continue
                args, _ = self._call_args(self.src, call.end() - 1)
                if args is None:
                    continue
                cparts = split_args(args)
                if len(cparts) < len(params):
                    continue
                values = dict(zip(params, cparts))
                text = body[1]
                for p, v in values.items():
                    text = re.sub(rf"\b{p}\b", v, text)
                self.expand_group_call(text, pname)

    def expand_group_call(self, text, pname):
        """Convert the group a procedure body builds."""
        m = re.search(r"\b(wbRArray|wbRArrayS|wbRStruct|wbRStructSK|wbRStructs|wbRStructsSK)\s*\(", text)
        if not m:
            return
        args, _ = self._call_args(text, m.end() - 1)
        if args is None:
            return
        parts = split_args(args)
        if not parts:
            return
        gname = pascal_string(parts[0]) or m.group(1)
        flat = {}
        blocks = {}
        self.convert_group(gname, m.group(1), parts, args, flat, blocks)
        # The group is held under the procedure's name, because that is how
        # a record refers to it.
        subs = []
        for sig, sub in flat.items():
            subs.append((sig, sub["name"], sub["type"], sub["note"]))
        for opener, block in blocks.items():
            for sig, sub in block["subrecords"].items():
                subs.append((sig, sub["name"], sub["type"], sub["note"]))
        if subs:
            self.local_groups[pname.lower()] = subs

    def expand_record_call(self, text):
        """Convert a `wbRecord`/`wbRefRecord` call found in an expanded body."""
        m = re.search(r"\b(wbRecord|wbRefRecord)\s*\(", text)
        if not m:
            return
        args, _ = self._call_args(text, m.end() - 1)
        if args is None:
            return
        parts = split_args(args)
        if len(parts) < 3:
            return
        sig = parts[0].strip()
        name = pascal_string(parts[1]) or ""
        members_at = 2
        header_flags = None
        if parts[2].lstrip().startswith(("wbFlags", "wbFormaterUnion")):
            members_at = 3
            header_flags = self.convert_flags(parts[2])
        if len(parts) <= members_at:
            return
        members = bracket_list(parts[members_at])
        if members is None:
            return
        rec = self.convert_record(sig, name, members, header_flags)
        if rec is not None:
            rec["note"] = f"{SOURCE}, {m.group(1)}({sig})"
            self.records[sig] = rec

    def _procedure_body(self, open_at):
        """A procedure's parameter list and body, as text.

        `open_at` is the position of the procedure's name; the parameter
        list runs from the `(` that follows it to the `)` that ends it.
        """
        # The parameter list runs to the `)` that ends it.
        open_paren = self.src.find("(", open_at)
        if open_paren < 0:
            return None
        depth = 0
        i = open_paren
        n = len(self.src)
        while i < n:
            c = self.src[i]
            if c == "'":
                i = skip_string(self.src, i)
                continue
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        params = self.src[open_at + 1:i]
        # The body runs to the `end;` that closes the procedure.
        start = self.src.find("begin", i)
        if start < 0:
            return None
        # The `begin` that opens the body is not counted: the `end;` that
        # closes the procedure is the one at depth zero.
        j = start + 5
        depth = 0
        while j < n:
            word = self.src[j:j + 5].lower()
            if word == "begin" or word == "case ":
                depth += 1
                j += 5
                continue
            if self.src[j:j + 3].lower() == "end":
                # `end;` closes the procedure unless it closes a `begin`.
                # The word must stand alone: "Blend" ends in "end" but is
                # not a keyword.
                before_ok = j == 0 or not (self.src[j - 1].isalnum() or self.src[j - 1] == "_")
                after = self.src[j + 3:j + 4]
                if after == ";":
                    if not before_ok:
                        j += 3
                        continue
                    if depth == 0:
                        return params, self.src[start:j]
                    depth -= 1
                    j += 4
                    continue
                if after in (" ", "\t", "\n", "\r"):
                    if not before_ok:
                        j += 3
                        continue
                    if depth == 0:
                        return params, self.src[start:j]
                    depth -= 1
                    j += 3
                    continue
            j += 1
        return None

    def convert_flags(self, text):
        """A record's header-flag list: the bits xEdit names for that type.

        The numbers in xEdit's comments are the *bit* numbers, so the mask is
        `1 << bit`.

        A `wbFormaterUnion` holds several such lists — xEdit picks the right
        one by the type of object the reference places — and the bits of all
        of them are the ones the record type may carry, so they are merged.
        """
        name, args, _ = match_call(text)
        if name is None or args is None:
            return None
        parts = split_args(args)
        if len(parts) < 2:
            return None
        if name == "wbFormaterUnion":
            # The members are the union's variants, each a flags list.
            members = bracket_list(parts[1]) if len(parts) > 1 else None
            if members is None:
                return None
            bits = {}
            for m in members:
                sub = self.convert_flags(m)
                if sub:
                    bits.update(sub["bits"])
            if not bits:
                return None
            return {
                "bits": bits,
                "confidence": "documented",
                "note": f"{SOURCE}, the record's header flags",
            }
        entries = bracket_list(parts[1])
        if entries is None:
            return None
        bits = {}
        for e in entries:
            # `{0x00000002}  2, 'Never Fades'` — the mask, the bit, the name.
            m = re.match(r"\{(0x[0-9A-Fa-f]+)\}\s*(\d+)\s*,\s*'([^']*)'", e)
            if not m:
                continue
            bit = int(m.group(2))
            label = m.group(3)
            # A bit xEdit calls "Unknown N" is left out, as the repo's rule
            # for header flags says.
            if label.lower().startswith("unknown"):
                continue
            bits[str(bit)] = label
        if not bits:
            return None
        return {
            "bits": bits,
            "confidence": "documented",
            "note": f"{SOURCE}, wbFlagsList for the record's header flags",
        }

    # -- members ------------------------------------------------------------

    def convert_member(self, kind, args, ctx):
        """Convert one definition to a schema type. Returns (type, note)."""
        parts = split_args(args)

        if kind == "wbInteger":
            return self._convert_integer(parts)
        if kind == "wbFloat":
            return "f32", self._note(ctx)
        if kind in ("wbFormIDCk", "wbFormIDCkNoReach", "wbFormIDCK", "wbFormIDck", "wbFormID"):
            return "formid", self._note(ctx)
        if kind in ("wbString", "wbStringKC", "wbStringLC", "wbStringT", "wbStringForward"):
            return self._convert_string(parts)
        if kind in ("wbLString", "wbLStringKC"):
            return "localized", self._note(ctx)
        if kind == "wbLenString":
            # `wbLenString(name, size)` — the size is the width of the
            # length prefix. Fallout 4's are all two bytes, where the
            # other games' are four.
            size = parts[1].strip() if len(parts) > 1 else "0"
            if size == "2":
                return "len_string_u16", self._note(ctx)
            return "len_string", self._note(ctx)
        if kind == "wbByteArray":
            return self._convert_byte_array(parts)
        if kind == "wbUnknown":
            return "bytes", self._note(ctx)
        if kind == "wbEmpty":
            return {"struct": []}, self._note(ctx)
        if kind == "wbNull":
            # A variant with no layout of its own: the case exists, and
            # nothing is read for it.
            return {"struct": []}, self._note(ctx)
        if kind in ("wbStruct", "wbStructSK", "wbStructExSK"):
            return self._convert_struct(parts, ctx)
        if kind in ("wbStructs", "wbStructsSK"):
            # `wbStructs(SIG, name, kind, [members])` is one subrecord of
            # that signature holding a struct of that kind, and
            # `wbStructs(name, kind, [members])` is the same without a
            # signature. Either way the layout is a struct.
            return self._convert_struct(parts, ctx)
        if kind in ("wbArray", "wbArrayS"):
            return self._convert_array(parts, ctx)
        if kind == "wbUnion":
            return self._convert_union(parts, ctx)
        if kind in ("wbEnum", "wbFlags"):
            # A formatter, not a layout — unless the unit names it, in which
            # case it is a definition of its own: `wbLocationEnum` is the
            # unit's list of what a package's location may be.
            values = self._formatter_values(f"{kind}({args})")
            if values is None:
                return None, None
            table_kind, table = values
            if table_kind == "enum":
                return (
                    {"enum": "u32", "values": {str(k): v for k, v in table.items()}},
                    self._note(ctx),
                )
            return (
                {"flags": "u32", "bits": {str(k): v for k, v in table.items()}},
                self._note(ctx),
            )
        # A name the unit declares is a shared definition, and wins over the
        # built-in helper expansion.
        ty, note = self._lookup_type(kind)
        if ty is not None:
            return ty, note
        if kind in HELPERS:
            return self._helper_struct(kind, parts)
        # A definition the hand-written files provide: the generator emits a
        # reference to it by name, and the hand-written file gives it its
        # layout. `wbMODT` is one: xEdit's own layout for it is conditional
        # on a flag, and the measured one is written out.
        if kind in self.OVERRIDDEN:
            return (
                self.OVERRIDDEN[kind],
                self._note(f"{kind}, laid out in the hand-written file"),
            )
        self.problems.append(f"{ctx}: unhandled {kind}")
        return None, None

    def _sig_key(self, text):
        """The key a signature is written under, from the unit's declaration."""
        return sig_key(text, self.declared_sigs)

    def _note(self, ctx):
        return f"{SOURCE}, {ctx}"

    def _helper_struct(self, kind, parts):
        """A helper the .pas defines for itself, expanded to its layout.

        A helper may be called with a signature — `wbAmbientColors(DALC)` —
        in which case it is a subrecord, or with only a name, in which case
        it is a struct nested in a field.
        """
        sig = None
        name = None
        for p in parts:
            if is_signature(p):
                sig = p
            else:
                s = pascal_string(p)
                if s is not None:
                    name = s
        fields = [{"name": n, "type": t} for n, t in HELPERS[kind]]
        if sig:
            # A subrecord: the helper's own name is the field's name.
            return ({"struct": fields}, self._note(f"{kind}({sig})"))
        if name:
            return ({"struct": fields}, self._note(f"{kind}('{name}')"))
        return {"struct": fields}, self._note(kind)

    def _convert_string(self, parts):
        """A string, which may have a fixed length.

        xEdit's `wbString(sig, name, size, ...)` gives a fixed length when
        the size is not zero: the text is exactly that many bytes, with no
        terminator. `DIAL`'s `SNAM` is four bytes of tag — "HELO", "RUMO" —
        rather than a name.
        """
        size = None
        if len(parts) >= 3 and is_signature(parts[0].strip()):
            size = parts[2].strip()
        else:
            # The size is the argument that is a plain number: the form
            # without a signature may carry a flag first — `wbString(True,
            # 'Mesh', 260)` — so the number is found rather than counted.
            for p in parts[1:]:
                p = p.strip()
                if re.fullmatch(r"\d+", p):
                    size = p
                    break
            if size is None and len(parts) >= 2:
                size = parts[1].strip()
        try:
            n = int(size) if size is not None else 0
        except ValueError:
            n = 0
        if n > 0:
            # A fixed-length string: the bytes are text, with no terminator.
            return (
                {"array": "u8", "count": n},
                self._note(parts[0].strip()),
            )
        return "zstring", self._note(parts[0].strip())

    def _convert_integer(self, parts):
        # `wbInteger` has two forms: with a signature
        # (`wbInteger(SIG, name, intType, ...)`) and without one
        # (`wbInteger(name, intType, ...)`), so the type is the argument
        # after the name either way. The formatter is the argument after
        # that: an enum names its values, and a flag set names its bits.
        if len(parts) >= 3 and is_signature(parts[0].strip()):
            it = parts[2].strip()
            note_ctx = parts[0].strip()
            fmt = parts[3].strip() if len(parts) > 3 else ""
        elif len(parts) >= 2:
            it = parts[1].strip()
            note_ctx = parts[0].strip()
            fmt = parts[2].strip() if len(parts) > 2 else ""
        else:
            return "u32", self._note(parts[0] if parts else "?")
        ty = INT_TYPES.get(it, "u32")
        values = self._formatter_values(fmt)
        if values is not None:
            kind, table = values
            if kind == "enum":
                return (
                    {"enum": ty, "values": {str(k): v for k, v in table.items()}},
                    self._note(note_ctx),
                )
            return (
                {"flags": ty, "bits": {str(k): v for k, v in table.items()}},
                self._note(note_ctx),
            )
        return ty, self._note(note_ctx)

    def _formatter_values(self, fmt):
        """The values an enum or a flag set gives, as number -> name.

        The formatter is either the call itself — `wbEnum([...])` — or the
        name of a definition the unit declares — `wbLocationEnum` — in
        which case the definition's own values are used.
        """
        if not fmt:
            return None
        name, args, _ = match_call(fmt)
        if name is None:
            # A named definition the unit declares.
            ty = self.types.get(fmt.strip()) or self.types.get(self._canonical(fmt.strip()))
            if isinstance(ty, dict):
                if "values" in ty and "enum" in ty:
                    return "enum", {int(k): v for k, v in ty["values"].items()}
                if "bits" in ty and "flags" in ty:
                    return "flags", {int(k): v for k, v in ty["bits"].items()}
            return None
        if name not in ("wbEnum", "wbFlags") or args is None:
            return None
        members = bracket_list(args)
        if members is None:
            return None
        kind = "enum" if name == "wbEnum" else "flags"
        table = {}
        n = 0
        # A member may carry an explicit value in front of the name: the
        # two are separate arguments — `Int64($864804BE), 'BSOverdrive'` —
        # and the value is the number the field holds.
        #
        # The members are read from the text rather than from the split
        # arguments, because the split separates the value from its name
        # and the value may sit in a list of its own — `wbEnum([], [...])`.
        text = args
        # The list of members is the last bracketed list in the call.
        start = text.rfind("[")
        end = text.rfind("]")
        if start >= 0 and end > start:
            text = text[start + 1 : end]
        # The members are read as the pairs of a value and a name that
        # appear in the text, in order.
        pattern = re.compile(
            r"(?:\b(?:Int64|Int32|UInt64|UInt32)\s*\(\s*\$([0-9A-Fa-f]+)\s*\)|"
            r"\{\s*(\d+)\s*\})\s*,?\s*"
            r"'((?:[^']|'')*)'"
        )
        found = False
        for vm in pattern.finditer(text):
            hexval, brace, name = vm.groups()
            if hexval is not None:
                idx = int(hexval, 16)
            elif brace is not None:
                idx = int(brace)
            else:
                idx = n
            table[idx] = name.replace("''", "'")
            n = idx + 1
            found = True
        if found:
            return kind, table
        for m in members:
            m = m.strip()
            # A member may carry an explicit index: `{3} 'Cave'`.
            idx = None
            im = re.match(r"^\{(\d+)\}\s*(.*)$", m, re.S)
            if im:
                idx = int(im.group(1))
                m = im.group(2).strip()
            text = pascal_string(m)
            if text is None:
                continue
            table[idx if idx is not None else n] = text
            n = idx + 1 if idx is not None else n + 1
        if not table:
            return None
        return kind, table

    def _convert_byte_array(self, parts):
        # `wbByteArray` has the same two forms as `wbInteger`, so the size
        # is the argument after the name either way.
        if len(parts) >= 3 and is_signature(parts[0].strip()):
            size = parts[2].strip()
            note_ctx = parts[0].strip()
        elif len(parts) >= 2:
            size = parts[1].strip()
            note_ctx = parts[0].strip()
        else:
            return "bytes", self._note(parts[0] if parts else "?")
        try:
            n = int(size)
        except ValueError:
            n = 0
        if n > 0:
            return {"array": "u8", "count": n}, self._note(note_ctx)
        # A size of 0 means "the rest of the field".
        return "bytes", self._note(note_ctx)

    def _convert_struct(self, parts, ctx):
        # `wbStruct` has two forms: with a signature (`wbStruct(SIG, name,
        # [members])`) and without one (`wbStruct(name, [members])`). The
        # members are the argument that is a `[ ... ]` list either way.
        # `wbStructSK` adds a sort key before the name, which shifts the
        # members one place along; the list is still found by its shape:
        # a sort key is a list of integers, not of definitions.
        members = None
        at = -1
        for i, p in enumerate(parts[1:], 1):
            candidate = bracket_list(p)
            if candidate and self._is_member_list(candidate):
                members = candidate
                at = i
                break
        if members is None:
            return "bytes", self._note(ctx)
        fields = []
        for m in members:
            f = self.convert_field(m, ctx)
            if f is not None:
                fields.append(f)
        if not fields:
            return {"struct": []}, self._note(ctx)
        fields = self._resolve_direct_switches(fields, ctx)
        # A struct may be shorter than its fields, in two ways xEdit says:
        # the struct is marked optional, or it names a version, in which
        # case the fields a later version added are absent from an earlier
        # record. The schema cannot say which version a record is, so the
        # trailing fields are read as long as the data lasts.
        if self._struct_is_optional(parts, at) or self._struct_version(parts, at) is not None:
            for f in fields:
                f["optional"] = True
        return {"struct": fields}, self._note(ctx)

    def _struct_version(self, parts, at):
        """The version a struct names, if it names one.

        The arguments after the member list are the options, then a flag
        saying the struct may be absent, then a flag saying it is not
        written, then the version.
        """
        if at < 0:
            return None
        for p in parts[at + 1:]:
            p = p.strip()
            if re.fullmatch(r"\d+", p):
                return int(p)
        return None

    def _struct_is_optional(self, parts, at):
        """Whether xEdit says a struct may be shorter than its fields.

        The arguments after the member list are the options, then a flag
        saying the struct may be absent, then the version.
        """
        if at < 0:
            return False
        flag = parts[at + 2].strip() if len(parts) > at + 2 else ""
        return flag.lower() == "true"

    def _resolve_direct_switches(self, fields, ctx):
        """Finish a union that switches on a field of the same struct.

        Two things are settled here. A union that xEdit decides by a
        decider reading the struct's own field — `wbTypeDecider` reading
        the field's value as the variant's index — gets its cases from the
        switch field's own values, so the switch reads the same numbers
        the enum gives. And the switch field is marked so that its value
        is kept, which is what the union reads.
        """
        by_name = {f.get("name"): f for f in fields if isinstance(f, dict)}
        for f in fields:
            if not isinstance(f, dict):
                continue
            ty = f.get("type")
            if not isinstance(ty, dict):
                continue
            direct = ty.pop("_direct", False)
            case_of = ty.pop("_case_of", None)
            switch = ty.get("switch_on")
            if not switch:
                continue
            target = by_name.get(switch)
            if target is None:
                # The switch names a field of a sibling subrecord, which is
                # settled when the record is built.
                continue
            if direct:
                values = self._enum_values(target.get("type"))
                if not values:
                    continue
                n = len(ty.get("union", []))
                # The switch reads the enum's value, which the decoder
                # renders as the number the enum gives, so the case's key
                # is that number and the value is the variant.
                ty["cases"] = {str(k): k for k in values if k < n}
                ty["default"] = 0
            # The switch field's value is what the union reads, so it is
            # kept.
            target["expose"] = True
        return fields

    def _enum_values(self, ty):
        """The values of an enum field, as number -> name."""
        if isinstance(ty, dict) and "values" in ty and "enum" in ty:
            out = {}
            for k, v in ty["values"].items():
                try:
                    out[int(k)] = v
                except (TypeError, ValueError):
                    continue
            return out
        if isinstance(ty, str):
            named = self.types.get(ty) or self.types.get(self._canonical(ty))
            if isinstance(named, dict):
                return self._enum_values(named)
        return {}

    def convert_field(self, text, ctx):
        """A member of a struct: a named field, with no signature."""
        text = text.strip()
        # A member may carry a case index: `{0} wbInteger('X', itS16)`.
        text = re.sub(r"^\s*\{\d+\}\s*", "", text)
        # A bare name is a shared definition used as a field — `wbScriptEntry`
        # holds `wbScriptFlags` — so it is a field whose type is that
        # definition, not a field named after it.
        if re.fullmatch(r"wb[A-Za-z0-9_]*", text) and text in self.types:
            return {"name": text, "type": text}
        if re.fullmatch(r"wb[A-Za-z0-9_]*", text):
            # A name that is not yet collected: it is defined further down the
            # unit, so it is referred to by name and resolved when the schema
            # is written. The reference may differ in case from the
            # declaration, so the canonical spelling is recorded with it.
            if text.lower() == "wbunknown":
                # xEdit's own "an opaque field" marker.
                return {"name": "unknown", "type": "bytes"}
            if text in HELPERS:
                # A helper used as a bare field — `wbAmbientColors` inside a
                # struct — is a field whose type is the helper's layout.
                schema, _ = self.convert_member(text, "", f"{ctx}.{text}")
                if schema is None:
                    return None
                return {"name": text, "type": schema}
            ty, _ = self._lookup_type(text)
            if ty is not None:
                # The reference may differ in case from the declaration, so
                # the canonical spelling is used.
                return {"name": text, "type": self._canonical(text)}
            self.deferred.add(text)
            return {"name": text, "type": text}
        name, args, _ = match_call(text)
        if name is None:
            # A bare helper name — `wbAmbientColors` inside a struct — is a
            # field whose type is the helper's layout.
            if re.fullmatch(r"wb[A-Za-z0-9_]*", text) and text in HELPERS:
                schema, _ = self.convert_member(text, "", f"{ctx}.{text}")
                if schema is None:
                    return None
                return {"name": text, "type": schema}
            self.problems.append(f"{ctx}: unparsed field {text[:40]!r}")
            return None
        if name == "wbUnknown":
            return {"name": "unknown", "type": "bytes"}
        parts = split_args(args) if args is not None else []
        if not parts:
            return None
        # The field's name is the first quoted argument. Some of xEdit's
        # definitions put a flag before it — `wbString(True, 'Mesh', 260)` —
        # so the name is found rather than counted.
        fname = None
        for p in parts:
            fname = pascal_string(p)
            if fname is not None:
                break
        if fname is None:
            # A helper used as a bare field — `wbAmbientColors` inside a
            # struct — is a field whose type is the helper's layout, and
            # whose name is the helper's own.
            if name in HELPERS:
                schema, _ = self.convert_member(name, args, f"{ctx}.{name}")
                if schema is None:
                    return None
                return {"name": name, "type": schema}
            return None
        schema, _ = self.convert_member(name, args, f"{ctx}.{fname}")
        if schema is None:
            return None
        return {"name": fname, "type": schema}

    def _counter_field(self, parts):
        """The sibling field an array's count comes from, if it does.

        xEdit's arrays sometimes take their count from a callback rather
        than from the data — `wbOMODDataIncludeCounter` reads the struct's
        own "Include Count" field — and the schema language's `count_field`
        says the same thing. The callback's body is read for the field it
        names. A callback that counts the set bits of a field says so, and
        the schema's `count_bits` says it.
        """
        for p in parts:
            p = p.strip()
            if not re.fullmatch(r"wb[A-Za-z0-9_]*", p):
                continue
            d = self.counters.get(p.lower())
            if d:
                return d
        return None

    def _collect_counters(self):
        """The sibling field each count callback reads.

        A callback either reads a field's value — `Container.ElementNative
        // Values['Include Count']` — or counts the set bits of a field's
        // value, which is how xEdit counts a script's fragments: one for
        // each of the first three of the script's flags that is set.
        """
        out = {}
        for m in re.finditer(
            r"function\s+(\w*Counter)\s*\([^)]*\)\s*:\s*Cardinal\s*;(.*?)\nend;",
            self.src,
            re.S,
        ):
            body = m.group(2)
            # The field the callback reads: it asks the container for the
            # element's native value by name.
            fm = re.search(
                r"Element(?:NativeValues|ByName)\['([^']+)'\]", body
            )
            if not fm:
                continue
            name = fm.group(1)
            # The number of bits it counts, if it counts bits: the loop
            # `for i := 0 to 2` says how many.
            lm = re.search(r"for\s+i\s*:=\s*0\s+to\s+(\d+)\s+do", body, re.I)
            if lm and re.search(r"F\s+and\s+1", body):
                out[m.group(1).lower()] = {"field": name, "bits": int(lm.group(1)) + 1}
            else:
                out[m.group(1).lower()] = {"field": name, "bits": None}
        return out

    def _convert_array(self, parts, ctx):
        # `wbArray` has the same two forms as `wbStruct`: with a signature
        # (`wbArray(SIG, name, element, count)`) and without one
        # (`wbArray(name, element, count)`).
        elem_text = None
        count_text = "0"
        if len(parts) >= 4 and is_signature(parts[0].strip()):
            elem_text = parts[2].strip()
            count_text = parts[3].strip()
        elif len(parts) >= 3:
            elem_text = parts[1].strip()
            count_text = parts[2].strip()
        if elem_text is None:
            return "bytes", self._note(ctx)
        elem = self._convert_element(elem_text, ctx)
        if elem is None:
            return "bytes", self._note(ctx)
        try:
            count = int(count_text)
        except ValueError:
            count = 0
        if count > 0:
            return {"array": elem, "count": count}, self._note(ctx)
        # The count may come from a sibling field rather than from the
        # data, which the schema language's `count_field` says.
        counter = self._counter_field(parts)
        if counter:
            field = counter["field"]
            if counter.get("bits") is not None:
                return (
                    {"array": elem, "count_bits": [field, counter["bits"]]},
                    self._note(ctx),
                )
            return (
                {"array": elem, "count_field": field},
                self._note(ctx),
            )
        if count < 0:
            # A negative count means the count is in the data, and its width
            # is what the count's own value says: xEdit's -1 is a four-byte
            # count, -2 a two-byte one and -4 a single byte [measured: the
            # navigation map's merged-mesh array, whose count is -1, is
            # preceded by a four-byte count, and the script array, whose
            # count is -2, by a two-byte one].
            width = {1: "u32", 2: "u16", 4: "u8"}.get(-count)
            if width:
                return (
                    {"array": elem, "count_prefix": width},
                    self._note(ctx),
                )
        # A count of 0 means "fill the rest of the field".
        return {"array": elem}, self._note(ctx)

    def _convert_element(self, text, ctx):
        """The element of an array: a value definition, with no signature."""
        text = text.strip()
        # A bare name is a named type — `wbArray(BNAM, 'Blur Radius',
        # wbTimeInterpolator)` names the unit's own definition of it.
        if re.fullmatch(r"wb[A-Za-z0-9_]*", text):
            ty, _ = self._lookup_type(text)
            if ty is not None:
                return ty
            # A name defined further down the unit: referred to by name, and
            # resolved when the schema is written.
            self.deferred.add(text)
            return text
        name, args, _ = match_call(text)
        if name is None:
            self.problems.append(f"{ctx}: unparsed array element {text[:40]!r}")
            return None
        schema, _ = self.convert_member(name, args, ctx)
        return schema

    def _convert_union(self, parts, ctx):
        # `wbUnion` has the same two forms as `wbStruct`: with a signature
        # (`wbUnion(SIG, name, decider, [members])`) and without one
        # (`wbUnion(name, decider, [members])`).
        members = None
        decider = None
        for p in parts[1:]:
            if members is None:
                found = bracket_list(p)
                if found is not None:
                    members = found
                    continue
            # The decider's name: xEdit's deciders are named `wb...Decider`,
            # and a few are named `wbDeciderFormVersion99`.
            if decider is None and re.fullmatch(r"wb\w*[Dd]ecider\w*", p.strip()):
                decider = p.strip()
        if members is None:
            return "bytes", self._note(ctx)
        variants = []
        # Each variant's case index, which is not its place in the list: a
        # variant xEdit gives no layout — `wbNull` — is dropped, and the
        # ones after it keep their own indices.
        case_of = []
        for m in members:
            # A variant may carry a case index: `{0} wbFormIDCk(...)`.
            m = m.strip()
            idx = None
            im = re.match(r"^\{(\d+)\}\s*(.*)$", m, re.S)
            if im:
                idx = int(im.group(1))
                m = im.group(2).strip()
            name, args, _ = match_call(m)
            if name is None:
                # A member that is a bare name — `wbNull` — is a definition
                # of its own, and reads nothing.
                if not re.fullmatch(r"wb[A-Za-z0-9_]*", m):
                    continue
                schema, _ = self.convert_member(m.strip(), "", f"{ctx}.{m.strip()}")
                if schema is None:
                    case_of.append(idx)
                    continue
                size = self._type_size(schema)
                v = {"type": schema}
                if size is not None:
                    v["size"] = size
                variants.append(v)
                case_of.append(idx)
                continue
            if args is None:
                # A member with no arguments of its own — `wbNull` — is a
                # variant that reads nothing.
                schema, _ = self.convert_member(name, "", f"{ctx}.{name}")
                if schema is None:
                    case_of.append(idx)
                    continue
                size = self._type_size(schema)
                v = {"type": schema}
                if size is not None:
                    v["size"] = size
                variants.append(v)
                case_of.append(idx)
                continue
            sub = split_args(args) if args is not None else []
            if not sub:
                continue
            schema, _ = self.convert_member(name, args, f"{ctx}.{sub[0]}")
            if schema is None:
                # A variant with no layout of its own — `wbNull` — is the
                # empty one, and the case it stands for still exists.
                case_of.append(idx)
                continue
            size = self._type_size(schema)
            v = {"type": schema}
            if size is not None:
                v["size"] = size
            variants.append(v)
            case_of.append(idx)
        if not variants:
            return "bytes", self._note(ctx)
        union: dict = {"union": variants}
        # A union with a decider switches on a sibling's value rather than on
        # the bytes left. The decider's body says which sibling and which
        # value picks which variant.
        if decider:
            d = self.deciders.get(decider.lower())
            if d and d.get("switch"):
                # The decider's result is the variant's place in the list,
                # which is xEdit's own order, so no mapping is needed.
                if d.get("direct"):
                    # The switch's own value is the variant's index, so the
                    # cases are the switch field's own values, which the
                    # struct's own fields give.
                    union["switch_on"] = d["switch"]
                    union["_direct"] = True
                    union["_case_of"] = case_of
                elif d.get("bits"):
                    # A bit test: the first mask that is set picks the
                    # variant. The masks are keyed as they are written, so
                    # the order xEdit tests them is kept.
                    union["switch_on"] = d["switch"]
                    union["bits"] = {
                        f"{mask:#x}": variant
                        for mask, variant in d["bits"]
                        if variant < len(variants)
                    }
                    union["default"] = 0
                elif d.get("cases"):
                    union["switch_on"] = d["switch"]
                    union["cases"] = {
                        k: v for k, v in d["cases"].items() if v < len(variants)
                    }
                    union["default"] = 0
                elif d.get("min_version") is not None:
                    # The record's own format version decides: the later
                    # variant when the version is at least the minimum.
                    union["min_version"] = d["min_version"]
                else:
                    # The decider may name the variant to use when no case
                    # matches: `if i <> 0 then Result := 1` puts the later
                    # variant to use for every value but zero.
                    union["default"] = d.get("default", 0)
        return union, self._note(ctx)

    # -- a record -----------------------------------------------------------

    def convert_record(self, sig, name, members, header_flags):
        """A record: its subrecords, and the groups among them.

        A group of one repeating subrecord is a plain list of that subrecord,
        so it needs no construct of its own — the subrecord's definition is
        enough. A group of several subrecords in a fixed order is positional,
        so it becomes a `sequence` block.
        """
        flat = {}
        blocks = {}
        for m in members:
            mname, args, _ = match_call(m)
            if mname is None:
                # A bare named subrecord definition — `wbDataPosRot` — is a
                # member whose signature the definition itself carries.
                mname = m.strip()
                args = ""
            parts = split_args(args) if args is not None else []
            first = parts[0].strip() if parts else ""
            # A group may open with a signature — `wbStructs(CVPA, 'Components',
            # 'Component', [...])` is the `CVPA` subrecord and the group
            # around it — so the kind settles it, not the first argument.
            # A `wbStructs` with a signature is the subrecord itself, and
            # not a group, so it does not settle it that way.
            if (
                mname in GROUP_KINDS
                and len(parts) > 1
                and not (mname in ("wbStructs", "wbStructsSK") and is_signature(first))
            ):
                self.convert_group(sig, mname, parts, args, flat, blocks)
                continue
            if not is_signature(first):
                if mname in GROUP_KINDS:
                    self.convert_group(sig, mname, parts, args, flat, blocks)
                    continue
                # A group a procedure builds — `wbMorphGroups('Male Morph
                # Groups')` — holds the subrecords of the group it names.
                local = self.local_groups.get(mname.lower())
                if local:
                    for first_s, sname, schema, note in local:
                        self._merge_sub(flat, [], first_s, sname, schema, note)
                    continue
                # A named group used as a member — `wbCTDAs` — holds the
                # subrecords of the group it names, so it becomes a block.
                named = self.named_groups.get(mname.lower())
                if named:
                    block = self._named_group_block(named)
                    if block is not None:
                        blocks[block["opener"]] = block
                    continue
                # A named definition that carries a signature is a subrecord.
                sig_of = self._named_type_signature(mname)
                if sig_of is None:
                    continue
                first = sig_of
                parts = [first]
            # The signature is written under its four bytes, so a signature
            # whose first byte is a control character is written as that
            # character.
            first = self._sig_key(first)
            schema, note = self.convert_member(mname, args, f"{sig}.{first}")
            if schema is None:
                continue
            sname = pascal_string(parts[1]) if len(parts) > 1 else first
            if sname is None:
                # A helper called with a signature — `wbAmbientColors(DALC)` —
                # has no name of its own; the signature names it.
                sname = first
            self._merge_sub(flat, [], first, sname or first, schema, note)
        if not flat and not blocks:
            return None

        rec = {
            "name": name or sig,
            "confidence": "documented",
            "note": f"{SOURCE}, wbRecord({sig})",
        }
        if header_flags:
            rec["header_flags"] = header_flags
        rec["subrecords"] = flat
        if blocks:
            # A signature that a block opens and that the record also has of
            # its own comes first: a lens flare's `DNAM` is the record's own
            # number before the flares and the flare's name after them.
            openers = set()
            for b in blocks.values():
                opener = b.get("opener")
                if opener:
                    openers.add(opener)
            for opener in openers:
                if opener in flat:
                    # The record's own meaning for the signature comes
                    # first, and the block opens on the next one.
                    for b in blocks.values():
                        if b.get("opener") == opener:
                            b["own_first"] = True
            rec["sequence"] = {"start": [], "blocks": blocks, "end": []}
        self._expose_switch_fields(rec)
        return rec

    def _resolve_sub_switches(self, rec, flat, blocks):
        """Settle a union that switches on a field of a sibling subrecord.

        xEdit's `wbPerkDATADecider` reads the type in the perk's PRKE, and
        the perk's effect data is a form id or a spell or an entry point
        according to that type. The switch names the field of the sibling
        — "PRKE.Type" — and the cases are that field's own values, which
        the sibling's own definition gives. The field is marked so that its
        value is kept.

        The switch may name the sibling subrecord itself, whose value is a
        number an enum gives a name to — xEdit's audio effect chain reads
        the kind of effect from the KNAM before it, and the decider's
        cases are the names that kind has.
        """
        groups = [flat] + [b.get("subrecords", {}) for b in blocks.values()]
        for group in groups:
            for sub in group.values():
                for ty in self._unions_of(sub.get("type")):
                    self._resolve_one_switch(ty, groups)

    def _unions_of(self, ty):
        """The unions a subrecord's own type holds, at any depth."""
        out = []

        def scan(node, depth=0):
            if depth > 6:
                return
            if isinstance(node, list):
                for v in node:
                    scan(v, depth + 1)
                return
            if not isinstance(node, dict):
                return
            if isinstance(node.get("union"), list):
                out.append(node)
            for v in node.values():
                scan(v, depth + 1)

        scan(ty)
        return out

    def _resolve_one_switch(self, ty, groups):
        """Settle one union's switch against the record's own subrecords."""
        switch = ty.get("switch_on")
        if not switch:
            return
        # The switch may name a field of a sibling subrecord, or the
        # sibling subrecord itself.
        if "." in switch:
            sub_sig, field = switch.split(".", 1)
        else:
            sub_sig, field = switch, None
        # The sibling's own definition, and the field within it.
        target_sub = None
        for g in groups:
            if sub_sig in g:
                target_sub = g[sub_sig]
                break
        if target_sub is None:
            return
        target_ty = target_sub.get("type")
        n = len(ty.get("union", []))
        if field is not None:
            field_def = None
            if isinstance(target_ty, dict) and "struct" in target_ty:
                for f in target_ty["struct"]:
                    if f.get("name") == field:
                        field_def = f
                        break
            if field_def is None:
                return
            values = self._enum_values(field_def.get("type"))
            if not values:
                return
            ty["cases"] = {str(k): k for k in values if k < n}
            ty["default"] = 0
            field_def["expose"] = True
            return
        # The switch names the sibling subrecord itself, whose own value is
        # a number that an enum gives a name to. The decider's cases are
        # those names, so they are turned into the numbers the enum gives.
        values = self._enum_values(target_ty)
        if not values:
            return
        by_name = {name: k for k, name in values.items()}
        cases = ty.get("cases")
        if not isinstance(cases, dict):
            return
        ty["cases"] = {
            str(by_name[key]): value
            for key, value in cases.items()
            if key in by_name and value < n
        }
        ty["default"] = 0
        target_sub["expose"] = True

    def _expose_switch_fields(self, rec):
        """Mark the siblings that a union switches on, so they are kept.

        A union's switch may name a field of the struct around it rather
        than of its own — xEdit's `wbScriptObjFormatDecider` reads the
        "Object Format" of the virtual machine adapter that holds the
        script property — so the field is looked for in the record's own
        definitions as well as in the struct the union is in.
        """
        targets = set()

        def scan(node):
            if isinstance(node, dict):
                sw = node.get("switch_on")
                if isinstance(sw, str):
                    targets.add(sw)
                for v in node.values():
                    scan(v)
            elif isinstance(node, list):
                for v in node:
                    scan(v)

        scan(rec)
        if not targets:
            return
        groups = [rec.get("subrecords", {})] + [
            b.get("subrecords", {}) for b in rec.get("sequence", {}).get("blocks", {}).values()
        ]
        for group in groups:
            for sig, sub in group.items():
                if sig in targets and "expose" not in sub:
                    sub["expose"] = True
                    sub["note"] = (
                        f"{sub.get('note', '')} The value of this subrecord decides "
                        f"which variant a later union takes."
                    ).strip()
        # A field of a struct that holds a union further in: the switch may
        # name it, and its value has to be kept.
        for target in targets:
            for group in groups:
                for sig, sub in group.items():
                    ty = sub.get("type")
                    if not isinstance(ty, dict):
                        continue
                    if "struct" not in ty:
                        continue
                    for f in ty["struct"]:
                        if f.get("name") == target and "expose" not in f:
                            f["expose"] = True
                            f["note"] = (
                                f"{f.get('note', '')} The value of this field decides "
                                f"which variant a union takes."
                            ).strip()

    def _put_block(self, blocks, opener, order, subs, note):
        """Add a block, merging it with a block that opens the same way.

        xEdit gives two groups of a record the same opening subrecord — a
        package's "Package Data" and its "Procedure Tree" both open with
        `ANAM` — but the schema language allows one block per opener. The
        two are merged: their orders are joined and their subrecords are
        taken together. The merge is sound where the layouts agree, which
        is the case for every pair the generator has met: `ANAM` is a string
        in both, and the signatures that differ are in only one of the two.
        """
        existing = blocks.get(opener)
        if existing is None:
            blocks[opener] = {
                "opener": opener,
                "order": list(order),
                "subrecords": dict(subs),
            }
            return
        for sig in order:
            if sig not in existing["order"]:
                existing["order"].append(sig)
        for sig, sub in subs.items():
            existing_sub = existing["subrecords"].get(sig)
            if existing_sub is None:
                existing["subrecords"][sig] = sub
                continue
            if existing_sub.get("type") == sub.get("type"):
                continue
            # The same signature means two things in the two groups — a
            # scene's phase opens with a two-byte type, and the group ends
            # with an empty marker of the same signature — so the two are
            # offered as alternatives and the bytes left decide which.
            variants = []
            for t in (existing_sub.get("type"), sub.get("type")):
                if t is None:
                    continue
                if not any(v.get("type") == t for v in variants):
                    size = self._type_size(t)
                    v = {"type": t}
                    if size is not None:
                        v["size"] = size
                    variants.append(v)
            existing["subrecords"][sig] = {
                "name": existing_sub.get("name", sig),
                "type": {"union": variants},
                "confidence": "documented",
                "note": (
                    f"{existing_sub.get('note', '')} The same signature is also "
                    f"defined differently in another group of this record, so "
                    f"the two are alternatives and the bytes left decide which."
                ).strip(),
            }
        existing["note"] = (
            f"{existing.get('note', note)} The group is also opened by another "
            f"group of the same record, so their subrecords are merged."
        )

    def convert_group(self, sig, kind, parts, args, flat, blocks):
        """A group: a run of subrecords that repeats, or a fixed run.

        A group's element may itself be a group — `ARMO`'s "Models" is a run
        of `INDX`/`MODL` pairs, each pair a group of its own — so the group's
        members are found by walking down to the innermost list.
        """
        gname = pascal_string(parts[0]) if parts else kind
        members = self._group_members(parts)
        if members is None:
            # A group of one repeating subrecord: the element is a single
            # definition rather than a list.
            self.convert_repeating(sig, gname, parts, flat, kind)
            return
        if members == "NESTED":
            # The group's element is itself a group; its members are one
            # level down.
            members = self._nested_members(sig, gname, parts)
            if not members:
                return
        if members == "NAMED":
            # The group's element is a named type that is itself a group —
            # `wbCTDAs` holds `wbCTDA`, which holds the `CTDA` subrecord. The
            # named type's own subrecords are the group's.
            named = self.named_groups.get(parts[1].strip().lower())
            if not named:
                return
            subs = {}
            order = []
            for first, sname, schema, note in named:
                subs[first] = {
                    "name": sname,
                    "type": schema,
                    "confidence": "documented",
                    "note": note,
                }
                order.append(first)
            self._put_block(blocks, order[0], order, subs, "")
            return
        if members == "UNION":
            # The group's element is a union of alternative groups — a
            # landscape's layers are either a base layer or an alpha layer.
            # Each variant is a block of its own, opened by the signature
            # that starts it, because the variants do not start with the
            # same one: a base layer opens with BTXT and an alpha layer
            # with ATXT.
            for variant in self._union_variants(parts):
                subs = {}
                order = []
                for first, sname, schema, note in variant:
                    self._merge_sub(subs, order, first, sname, schema, note)
                if not subs:
                    continue
                self._put_block(blocks, order[0], order, subs, "")
            return

        # A fixed run of several subrecords: a sequence block.
        subs = {}
        order = []
        for first, sname, schema, note in self._group_subrecords(
            f"{sig}.{gname}", kind, parts
        ):
            self._merge_sub(subs, order, first, sname, schema, note)
        if not subs:
            return
        # The block's opener is the first signature it holds.
        self._put_block(blocks, order[0], order, subs, "")

    def _is_member_list(self, members):
        """Whether a bracketed list is a list of definitions, not a sort key.

        xEdit's `wbRStructsSK(name, kind, [sortkey], [members], ...)` puts a
        sort key — a list of integers — before the members, and the two are
        both bracketed lists. A sort key's entries are numbers, never
        definitions.
        """
        if not members:
            return False
        return all(
            (match_call(m)[0] is not None)
            or re.fullmatch(r"wb[A-Za-z0-9_]+", m.strip())
            for m in members
        )

    def _group_members(self, parts):
        """The group's member list, or None if it holds a single element.

        The string "NESTED" means the element is a group of its own, whose
        members are one level down. The string "NAMED" means the element is
        a named type that is itself a group. The string "UNION" means the
        element is a union of alternative groups.
        """
        for p in parts[1:]:
            members = bracket_list(p)
            if members is not None and self._is_member_list(members):
                return members
        # The element is a single definition. If it is itself a group, the
        # members are inside it.
        if len(parts) > 1:
            element = parts[1].strip()
            ename, eargs, _ = match_call(element)
            if ename in GROUP_KINDS and eargs is not None:
                inner = self._group_members(split_args(eargs))
                if inner is not None:
                    return "NESTED"
            elif ename == "wbRUnion" and eargs is not None:
                return "UNION"
            elif ename is None and self._is_named_group(element):
                return "NAMED"
        return None

    def _union_variants(self, parts):
        """The groups a `wbRUnion` holds, as lists of subrecords.

        A union of groups names alternative shapes — a landscape's layers are
        either a base layer or an alpha layer — and each variant is a group
        of its own.
        """
        out = []
        ename, eargs, _ = match_call(parts[1].strip())
        if ename is None or eargs is None:
            return out
        for p in split_args(eargs):
            variants = bracket_list(p)
            if variants is None:
                continue
            for v in variants:
                vname, vargs, _ = match_call(v)
                if vname is None or vargs is None:
                    continue
                subs = self._group_subrecords(
                    f"{parts[0]}.{vname}", vname, split_args(vargs)
                )
                if subs:
                    out.append(subs)
        return out

    def _is_named_group(self, name):
        """Whether a name is a named type whose own members are a group."""
        if not re.fullmatch(r"wb[A-Za-z0-9_]*", name):
            return False
        return name.lower() in self.named_groups

    def _named_group_members(self, sig, gname, parts):
        """The members of a group whose element is a named group."""
        element = parts[1].strip()
        members = self.named_groups.get(element.lower())
        if members is None:
            return None
        # The members are the named type's own; the subrecord definitions
        # they carry are already built, so they are taken whole.
        return members

    def _nested_members(self, sig, gname, parts):
        """The members of a group whose element is a group of its own."""
        ename, eargs, _ = match_call(parts[1])
        if ename is None or eargs is None:
            return None
        eparts = split_args(eargs)
        for p in eparts[1:]:
            members = bracket_list(p)
            if members is not None:
                return members
        return None

    def convert_repeating(self, sig, gname, parts, flat, kind="wbArray"):
        """A group of one repeating subrecord: a plain list of that subrecord."""
        if len(parts) < 2:
            self.problems.append(f"{sig}: group {gname} names no element")
            return
        element = parts[1].strip()
        ename, eargs, _ = match_call(element)
        if ename is None or eargs is None:
            # The element may be a named definition that owns a signature —
            # `wbPDTOs` repeats `wbPDTO`, which is the `PDTO` subrecord.
            sig_of = self._named_type_signature(element)
            if sig_of is None:
                self.problems.append(f"{sig}: group {gname} element {element[:40]!r}")
                return
            ty, note = self._lookup_type(element)
            if ty is None:
                return
            flat[sig_of] = {
                "name": sig_of,
                "type": ty,
                "confidence": "documented",
                "note": f"{note}. The group {gname!r} repeats this subrecord.",
            }
            return
        eparts = split_args(eargs)
        if not eparts or not is_signature(eparts[0]):
            self.problems.append(f"{sig}: group {gname} element is not a subrecord")
            return
        ssig = self._sig_key(eparts[0])
        schema, note = self.convert_member(ename, eargs, f"{sig}.{gname}.{ssig}")
        if schema is None:
            return
        if kind == "wbRArrayS" and schema == "zstring":
            # A restricted array of strings: each element is the four
            # bytes of a code that says which one it is — a race's
            # movement type names are "WALK", "RUN1", "SNEK", "BLDO" and
            # "SWIM", four bytes each and nothing else.
            schema = {"array": "u8", "count": 4}
        sname = pascal_string(eparts[1]) if len(eparts) > 1 else ssig
        flat[ssig] = {
            "name": sname or ssig,
            "type": schema,
            "confidence": "documented",
            "note": f"{note}. The group {gname!r} repeats this subrecord.",
        }


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    src_path = Path(sys.argv[1])
    out_dir = Path(sys.argv[2])
    c = Converter(src_path.read_text(encoding="utf-8", errors="replace")).run()

    skipped = {s for s in HAND_WRITTEN if s in c.records}
    records = {k: v for k, v in c.records.items() if k not in HAND_WRITTEN}
    # The overridden names are the hand-written files' own; the generator
    # only refers to them.
    types = {
        k: v for k, v in sorted(c.types.items())
        if k not in Converter.OVERRIDDEN
    }

    out_dir.mkdir(parents=True, exist_ok=True)
    doc = {
        "format": "fnv.schema/1",
        "types": types,
        "records": {k: records[k] for k in sorted(records)},
    }
    dest = out_dir / "generated.json"
    dest.write_text(json.dumps(doc, indent=1, sort_keys=True) + "\n")
    print(f"{dest}: {len(records)} records, {len(types)} types")
    if skipped:
        print(f"note: {len(skipped)} records are written by hand: {sorted(skipped)}")
    if c.problems:
        print(f"note: {len(c.problems)} members were not translated:")
        for p in sorted(set(c.problems))[:60]:
            print(f"  {p}")
    unresolved = sorted(c.deferred - set(c.types))
    if unresolved:
        print(f"note: {len(unresolved)} names are referred to but never defined:")
        for u in unresolved[:20]:
            print(f"  {u}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

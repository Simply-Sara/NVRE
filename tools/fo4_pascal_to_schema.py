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
    "wbRStructExSK", "wbRStructsSK", "wbRUnion", "wbStructs",
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
    """Whether `text` is a four-character record signature."""
    return bool(re.fullmatch(r"[A-Z][A-Z0-9_]{3}", text.strip()))


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
        self.records = {}        # signature -> record definition
        self.problems = []
        self.deferred = set()    # names referred to before they were defined

    def run(self):
        self.collect_named_types()
        self.collect_records()
        self.resolve_deferred()
        return self

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
                return p
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
                subs = self._group_subrecords(var, kind, split_args(args))
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
                # The member is a named definition that owns a signature.
                sig_of = self._named_type_signature(mname)
                if sig_of is None:
                    continue
                first = sig_of
                mparts = [first]
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
            body = self._procedure_body(m.end() - 1)
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
        """A procedure's parameter list and body, as text."""
        # The parameter list runs to the `);` that ends it.
        depth = 0
        i = open_at
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
        depth = 0
        j = start
        while j < n:
            word = self.src[j:j + 5].lower()
            if word == "begin" or word == "case ":
                depth += 1
                j += 5
                continue
            if self.src[j:j + 3].lower() == "end":
                # `end;` closes the procedure unless it closes a `begin`.
                after = self.src[j + 3:j + 4]
                if after == ";":
                    if depth == 0:
                        return params, self.src[start:j]
                    depth -= 1
                    j += 4
                    continue
                if after in (" ", "\t", "\n", "\r"):
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
            return "len_string", self._note(ctx)
        if kind == "wbByteArray":
            return self._convert_byte_array(parts)
        if kind == "wbUnknown":
            return "bytes", self._note(ctx)
        if kind == "wbEmpty":
            return {"struct": []}, self._note(ctx)
        if kind in ("wbStruct", "wbStructSK", "wbStructExSK"):
            return self._convert_struct(parts, ctx)
        if kind in ("wbArray", "wbArrayS"):
            return self._convert_array(parts, ctx)
        if kind == "wbUnion":
            return self._convert_union(parts, ctx)
        if kind in ("wbEnum", "wbFlags"):
            return None, None  # a formatter, not a layout
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
        elif len(parts) >= 2:
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
        # after the name either way.
        if len(parts) >= 3 and is_signature(parts[0].strip()):
            it = parts[2].strip()
            note_ctx = parts[0].strip()
        elif len(parts) >= 2:
            it = parts[1].strip()
            note_ctx = parts[0].strip()
        else:
            return "u32", self._note(parts[0] if parts else "?")
        ty = INT_TYPES.get(it, "u32")
        return ty, self._note(note_ctx)

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
        for p in parts[1:]:
            candidate = bracket_list(p)
            if candidate and all(
                (match_call(m)[0] is not None)
                or re.fullmatch(r"wb[A-Za-z0-9_]+", m.strip())
                for m in candidate
            ):
                members = candidate
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
        return {"struct": fields}, self._note(ctx)

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
        fname = pascal_string(parts[0])
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
        for p in parts[1:]:
            members = bracket_list(p)
            if members is not None:
                break
        if members is None:
            return "bytes", self._note(ctx)
        variants = []
        for m in members:
            # A variant may carry a case index: `{0} wbFormIDCk(...)`.
            m = re.sub(r"^\s*\{\d+\}\s*", "", m)
            name, args, _ = match_call(m)
            if name is None:
                continue
            sub = split_args(args) if args is not None else []
            if not sub:
                continue
            schema, _ = self.convert_member(name, args, f"{ctx}.{sub[0]}")
            if schema is not None:
                variants.append({"type": schema})
        if not variants:
            return "bytes", self._note(ctx)
        return {"union": variants}, self._note(ctx)

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
            if not is_signature(first):
                if mname in GROUP_KINDS:
                    self.convert_group(sig, mname, parts, args, flat, blocks)
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
            schema, note = self.convert_member(mname, args, f"{sig}.{first}")
            if schema is None:
                continue
            sname = pascal_string(parts[1]) if len(parts) > 1 else first
            if sname is None:
                # A helper called with a signature — `wbAmbientColors(DALC)` —
                # has no name of its own; the signature names it.
                sname = first
            flat[first] = {
                "name": sname or first,
                "type": schema,
                "confidence": "documented",
                "note": note,
            }
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
            rec["sequence"] = {"start": [], "blocks": blocks, "end": []}
        return rec

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
            self.convert_repeating(sig, gname, parts, flat)
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
            blocks[order[0]] = {
                "opener": order[0],
                "order": order,
                "subrecords": subs,
            }
            return

        # A fixed run of several subrecords: a sequence block.
        subs = {}
        order = []
        for m in members:
            mname, margs, _ = match_call(m)
            if mname is None:
                continue
            mparts = split_args(margs) if margs is not None else []
            if not mparts:
                continue
            first = mparts[0].strip()
            if not is_signature(first):
                # A nested group: the schema language has no nested block, so
                # it is noted rather than silently dropped.
                self.problems.append(f"{sig}: group {gname} has a nested group {mname}")
                continue
            schema, note = self.convert_member(mname, margs, f"{sig}.{gname}.{first}")
            if schema is None:
                continue
            sname = pascal_string(mparts[1]) if len(mparts) > 1 else first
            subs[first] = {
                "name": sname or first,
                "type": schema,
                "confidence": "documented",
                "note": note,
            }
            order.append(first)
        if not subs:
            return
        # The block's opener is the first signature it holds.
        blocks[order[0]] = {
            "opener": order[0],
            "order": order,
            "subrecords": subs,
        }

    def _group_members(self, parts):
        """The group's member list, or None if it holds a single element.

        The string "NESTED" means the element is a group of its own, whose
        members are one level down. The string "NAMED" means the element is
        a named type that is itself a group.
        """
        for p in parts[1:]:
            members = bracket_list(p)
            if members is not None:
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
            elif ename is None and self._is_named_group(element):
                return "NAMED"
        return None

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

    def convert_repeating(self, sig, gname, parts, flat):
        """A group of one repeating subrecord: a plain list of that subrecord."""
        if len(parts) < 2:
            self.problems.append(f"{sig}: group {gname} names no element")
            return
        element = parts[1].strip()
        ename, eargs, _ = match_call(element)
        if ename is None or eargs is None:
            self.problems.append(f"{sig}: group {gname} element {element[:40]!r}")
            return
        eparts = split_args(eargs)
        if not eparts or not is_signature(eparts[0]):
            self.problems.append(f"{sig}: group {gname} element is not a subrecord")
            return
        ssig = eparts[0].strip()
        schema, note = self.convert_member(ename, eargs, f"{sig}.{gname}.{ssig}")
        if schema is None:
            return
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

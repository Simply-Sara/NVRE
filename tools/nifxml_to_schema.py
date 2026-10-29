#!/usr/bin/env python3
"""Turn Bethesda's nif.xml into NVRE's schema JSON.

nif.xml is the format definition the NIF tooling is generated from. It says,
for every block and struct type, what its fields are: the name, the type,
how many of them there are, and the condition under which they are there.

NVRE's schema says the same things in its own words: a JSON file per layer,
with the fields as objects. This converts one into the other.

The conversion is not a copy, because the two languages differ:

- nif.xml names its fields "Num Data" and its versions "#FO4#"; NVRE names
  the field `num_data` and writes the version as a condition on `bs_version`.
  A *type* keeps its name in both: "NiBound" is "NiBound".
- nif.xml's conditions are written in its own expression language, with
  tokens like `#BS_FO4#` and operators like `#BITAND#`; NVRE's is a plain
  expression language over the fields read so far.
- nif.xml's types are named as its own code names them ("Vector3", "Ref");
  NVRE has primitives for most of them and structs for the rest.

Usage:
    nifxml_to_schema.py <nif.xml> <out-dir> [--game FO4|SSE|SKY|FO3]
                        [--only Type ...] [--blocks] [--structs] [--enums]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import xml.etree.ElementTree as ET

# The NIF versions a game uses, as the Bethesda version.
GAME_BS = {
    "FO3": 34,
    "SKY": 83,
    "SSE": 100,
    "FO4": 130,
    "F76": 155,
}

# The tokens nif.xml's expressions use, and what they mean in terms of the
# Bethesda version.
TOKEN_BS = {
    "#BS_FO4#": "bs_version == 130",
    "#BS_FO4_2#": "(bs_version >= 130 && bs_version <= 139)",
    "#BS_F76#": "bs_version == 155",
    "#BS_SSE#": "bs_version == 100",
    "#BS_GTE_SSE#": "bs_version >= 100",
    "#BS_GTE_SKY#": "bs_version >= 83",
    "#BS_GT_FO3#": "bs_version > 34",
    "#BS_GTE_FO3#": "bs_version >= 34",
    "#BS_GTE_130#": "bs_version >= 130",
    "#BS_GTE_132#": "bs_version >= 132",
    "#BS_GTE_152#": "bs_version >= 152",
    "#BS_GT_130#": "bs_version > 130",
    "#NI_BS_LT_FO3#": "bs_version < 34",
    "#NI_BS_LTE_FO3#": "bs_version <= 34",
    "#NI_BS_LT_FO4#": "bs_version < 130",
    "#NI_BS_LTE_FO4#": "bs_version <= 139",
    "#NI_BS_LT_SSE#": "bs_version < 100",
    "#NI_BS_LTE_16#": "bs_version <= 16",
    "#BSSTREAM#": "bs_version > 0",
    "#NISTREAM#": "bs_version == 0",
    "#BS202#": "bs_version > 0",
    "#DIVINITY2#": "(user_version == 0x20000 || user_version == 0x30000)",
    "#BSSTREAMHEADER#": "bs_version > 0",
}

# The operators nif.xml's expressions use, and NVRE's.
OPERATORS = {
    "#BITAND#": "&",
    "#BITOR#": "|",
    "#BITXOR#": "^",
    "#RSH#": ">>",
    "#LSH#": "<<",
    "#EQ#": "==",
    "#NEQ#": "!=",
    "#LT#": "<",
    "#LTE#": "<=",
    "#GT#": ">",
    "#GTE#": ">=",
    "#ADD#": "+",
    "#SUB#": "-",
    "#MUL#": "*",
    "#DIV#": "/",
    "#AND#": "&&",
    "#OR#": "||",
}

# The constants nif.xml's expressions use.
CONSTS = {
    "#FLT_MAX#": "3.402823466e+38",
    "#FLT_MIN#": "-3.402823466e+38",
    "#FLT_INF#": "1e308",
    "#UINT_MAX#": "0xFFFFFFFF",
    "#USHRT_MAX#": "0xFFFF",
    "#BYTE_MAX#": "0xFF",
    "#INT_MAX#": "2147483647",
    "#INT_MIN#": "-2147483648",
    "#CHAR_MAX#": "127",
    "#CHAR_MIN#": "-128",
}

# nif.xml's basic types, and NVRE's primitives.
PRIMS = {
    "uint": "u32",
    "int": "i32",
    "ushort": "u16",
    "short": "i16",
    "byte": "u8",
    "sbyte": "i8",
    "char": "u8",
    "bool": "bool",
    "float": "f32",
    "hfloat": "hfloat",
    "uint64": "u64",
    "int64": "u64",
    "ulittle32": "u32",
    "normbyte": "normbyte",
    "normsbyte": "normbyte",
    "Ref": "ref",
    "Ptr": "ptr",
    "StringOffset": "u32",
    "NiFixedString": "u32",
    "BlockTypeIndex": "u16",
    "FileVersion": "u32",
    # The names an enum's storage may be written as. nif.xml says
    # `storage="byte"`, but the enum's own name may also be used.
    "byte": "u8",
    "ushort": "u16",
    "uint": "u32",
}

# nif.xml's vector and matrix types, and NVRE's.
VECTORS = {
    "Vector2": "vec2",
    "Vector3": "vec3",
    "Vector4": "vec4",
    "Color3": "color3",
    "Color4": "color4",
    "ByteColor4": "byte_color4",
    "ByteColor3": "byte_color4",
    "ByteVector3": "byte_vec3",
    "SbyteVector3": "byte_vec3",
    # Three 16-bit numbers, as a vertex's position: a triangle's vertices
    # in a compressed mesh shape are stored this way, which is six bytes
    # and not the three a byte vector takes.
    "UshortVector3": "u16_vec3",
    "Quaternion": "quat",
    "Matrix22": "mat22",
    "Matrix33": "mat33",
    "Matrix34": "mat34",
    "Matrix44": "mat44",
    "Triangle": "triangle",
    "HalfVector2": "half_vec2",
    "HalfVector3": "half_vec3",
    "HalfColor4": "half_vec4",
    "HalfTexCoord": "half_vec2",
    "TexCoord": "tex_coord",
    "NiRGBA": "color4",
}


def snake(name: str) -> str:
    """The name a *field* gets in NVRE's schema."""
    name = name.replace("::", "__")
    name = re.sub(r"[^0-9A-Za-z_]+", "_", name)
    name = re.sub(r"_+", "_", name).strip("_")
    if not name:
        return "field"
    if name[0].isdigit():
        name = "f_" + name
    return name.lower()


def type_name(name: str) -> str:
    """The name a *type* gets in NVRE's schema.

    A type keeps the name it has in the file's block table, so that the
    schema can be looked up by the name the file uses — including the
    nested names the file writes with "::".
    """
    return name


def pack_version(text: str) -> int:
    """A dotted version, packed into the number the file's header holds.

    The parts are most significant first: "20.2.0.7" is 0x14020007.
    """
    parts = [int(p) for p in text.split(".")]
    while len(parts) < 4:
        parts.append(0)
    v = 0
    for p in parts[:4]:
        v = (v << 8) | (p & 0xFF)
    return v


class Converter:
    def __init__(self, xml_path: str, game: str):
        self.root = ET.parse(xml_path).getroot()
        self.game = game
        self.bs = GAME_BS[game]
        self.objects = {o.get("name"): o for o in self.root.findall("niobject")}
        self.structs = {s.get("name"): s for s in self.root.findall("struct")}
        self.enums = {e.get("name"): e for e in self.root.findall("enum")}
        self.bitflags = {b.get("name"): b for b in self.root.findall("bitflags")}
        self.bitfields = {b.get("name"): b for b in self.root.findall("bitfield")}
        self.skipped: list[tuple[str, str]] = []

    # -- the expressions ------------------------------------------------

    def convert_expr(self, text: str, names: dict[str, str]) -> str | None:
        """Turn one of nif.xml's expressions into NVRE's.

        `names` maps the XML's field names to NVRE's, so that a condition
        written against "Data Size" becomes one against `data_size`.

        Returns None if it cannot be done, in which case the field that
        holds it is not converted either: a field with the wrong condition
        is worse than a field that is not there.
        """
        if not text:
            return None
        out = text.strip()
        # The field names first, longest first, so that "Data Size" is
        # replaced before "Data".
        for xml_name in sorted(names, key=len, reverse=True):
            out = re.sub(
                rf"(?<![\w]){re.escape(xml_name)}(?![\w])",
                names[xml_name],
                out,
            )
        # The tokens, longest first so that "#BS_FO4#" does not match
        # inside "#BS_FO4_2#".
        for token in sorted(TOKEN_BS, key=len, reverse=True):
            out = out.replace(token, f"({TOKEN_BS[token]})")
        for token in sorted(OPERATORS, key=len, reverse=True):
            out = out.replace(token, OPERATORS[token])
        for token in sorted(CONSTS, key=len, reverse=True):
            out = out.replace(token, CONSTS[token])
        # The plain version names.
        out = out.replace("#VER#", "version")
        out = out.replace("#USER#", "user_version")
        out = out.replace("#BSVER#", "bs_version")
        out = out.replace("#ARG#", "arg")
        out = out.replace("#ARG1#", "arg")
        # "BS Version >= 103" is another way of writing the Bethesda
        # version.
        out = re.sub(
            r"BS\s+Version\s*(>=|<=|>|<|==|!=)\s*(\d+)",
            lambda m: f"bs_version {m.group(1)} {m.group(2)}",
            out,
        )
        # A dotted file version, as a number: the parts are packed into
        # one, most significant first, the way the file's header holds it.
        out = re.sub(
            r"\b(\d+)\.(\d+)\.(\d+)\.(\d+)\b",
            lambda m: str(
                (int(m.group(1)) << 24)
                | (int(m.group(2)) << 16)
                | (int(m.group(3)) << 8)
                | int(m.group(4))
            ),
            out,
        )
        # What is left is a token the conversion does not know.
        if re.search(r"#[A-Z0-9_]+#", out):
            return None
        # A name NVRE's expression language would not accept: two words
        # side by side, where nif.xml has a field name the conversion did
        # not know about.
        if re.search(r"(?<![\w.])([A-Za-z][A-Za-z0-9]*)\s+[A-Za-z]", out):
            return None
        return out.strip()

    # -- the types ------------------------------------------------------

    def field_type(self, xml_type: str) -> str | None:
        """The NVRE type a nif.xml field type becomes.

        An enum is read as the type it is stored as, which nif.xml says
        in the enum's own `storage` attribute — a byte for the Havok
        material types, a word for the shader flags, and so on. Getting
        this wrong shifts every field after it.
        """
        if xml_type in PRIMS:
            return PRIMS[xml_type]
        if xml_type in VECTORS:
            return VECTORS[xml_type]
        if xml_type == "string":
            return "string"
        if xml_type == "SizedString":
            return "sized_string"
        if xml_type == "SizedString16":
            return "sized_string16"
        if xml_type in self.enums or xml_type in self.bitflags:
            return type_name(xml_type)
        if xml_type in self.bitfields:
            return type_name(xml_type)
        if xml_type in self.structs or xml_type in self.objects:
            return type_name(xml_type)
        if xml_type == "#T#":
            return "T"
        return None

    # -- the fields -----------------------------------------------------

    def convert_field(
        self, f: ET.Element, owner: str, names: dict[str, str]
    ) -> dict | None:
        """One field, as NVRE writes it."""
        name = f.get("name")
        xml_type = f.get("type")
        if not name or not xml_type:
            return None
        ty = self.field_type(xml_type)
        if ty is None:
            self.skipped.append((owner, f"the type {xml_type!r} of {name!r}"))
            return None
        out: dict = {"name": snake(name), "type": ty}
        # The length: how many of them there are.
        length = f.get("length")
        if length:
            conv = self.convert_length(length, names)
            if conv is None:
                self.skipped.append((owner, f"the length {length!r} of {name!r}"))
                return None
            out["length"] = conv
        # The condition: when they are there.
        #
        # nif.xml has two: `cond` is about the block's own state, and
        # `vercond` is about the file's version. Both become conditions,
        # joined with "and" when both are there.
        conds = []
        for attr in ("cond", "vercond"):
            text = f.get(attr)
            if not text:
                continue
            conv = self.convert_expr(text, names)
            if conv is None:
                self.skipped.append((owner, f"the {attr} {text!r} of {name!r}"))
                return None
            conds.append(conv)
        # The file version the field appeared in, and left in.
        #
        # nif.xml writes a field's presence three ways: `since` and
        # `until` are the file version it appeared and left in, and
        # `ver1` and `ver2` are the range it is in.
        #
        # A version older than 20.0.0.0 is from before the format NVRE
        # reads, so the field is either always there or never there: a
        # `since` that old is dropped, and an `until` that old means the
        # field is not there at all.
        for attr in ("since", "until"):
            text = f.get(attr)
            if not text:
                continue
            if re.fullmatch(r"\d+\.\d+\.\d+\.\d+", text):
                out[attr] = text
            elif attr == "until":
                conds.append("false")
            # A `since` older than the format is already true, so it is
            # dropped.
        ver1 = f.get("ver1")
        ver2 = f.get("ver2")
        if ver1 or ver2:
            low = pack_version(ver1) if ver1 else None
            high = pack_version(ver2) if ver2 else None
            if low is not None:
                conds.append(f"version >= {low}")
            if high is not None:
                conds.append(f"version <= {high}")
        if conds:
            out["cond"] = " && ".join(f"({c})" for c in conds)
        # The template: what "T" stands for inside the field's type.
        #
        # It names a type, so it keeps the name the type has — but a
        # template may be a primitive, and NVRE writes those in its own
        # names.
        template = f.get("template")
        if template:
            if template == "#T#":
                out["template"] = "T"
            else:
                prim = PRIMS.get(template)
                out["template"] = prim if prim else type_name(template)
        # The argument: what the field's type is given.
        arg = f.get("arg") or f.get("arg1")
        if arg:
            conv = self.convert_expr(arg, names)
            if conv is None:
                self.skipped.append((owner, f"the arg {arg!r} of {name!r}"))
                return None
            out["arg"] = conv
        # The block types the field belongs to, and the ones it does not.
        #
        # nif.xml writes them as `onlyT` and `excludeT`: the field is read
        # only for a block of that type, or for every block but that one.
        only = f.get("onlyT")
        if only:
            out["only"] = [type_name(t) for t in only.split()]
        exclude = f.get("excludeT")
        if exclude:
            out["exclude"] = [type_name(t) for t in exclude.split()]
        # The width: how wide a row of a two-dimensional array is.
        width = f.get("width")
        if width:
            conv = self.convert_length(width, names)
            if conv is None:
                self.skipped.append((owner, f"the width {width!r} of {name!r}"))
                return None
            out["width"] = conv
        return out

    def convert_length(self, text: str, names: dict[str, str]) -> str | int | None:
        """Turn a length into NVRE's.

        A length is either a number, the name of an earlier field, or an
        expression. NVRE's schema writes the first as a number, the second
        as the field's name, and the third as an expression.
        """
        if text is None:
            return None
        text = text.strip()
        if re.fullmatch(r"\d+", text):
            return int(text)
        # A plain name: the name of an earlier field.
        if "#" not in text and re.fullmatch(r"[A-Za-z0-9]+(?: [A-Za-z0-9]+)*", text):
            return snake(text)
        return self.convert_expr(text, names)

    # -- the types ------------------------------------------------------

    def chain(self, elem: ET.Element) -> list[ET.Element]:
        """A type and the types it is based on, nearest first."""
        out = []
        seen = set()
        cur = elem
        while cur is not None:
            out.append(cur)
            base = cur.get("inherit")
            if not base or base in seen:
                break
            seen.add(base)
            cur = self.objects.get(base) or self.structs.get(base)
        return out

    def names_of(self, elem: ET.Element) -> dict[str, str]:
        """The field names a type's conditions are written against.

        A condition may name a field of the type or of any type it is
        based on, so the names of the whole chain are collected.
        """
        names: dict[str, str] = {}
        for e in self.chain(elem):
            for f in e.findall("field"):
                n = f.get("name")
                if n and n not in names:
                    names[n] = snake(n)
        return names

    def fields_of(self, elem: ET.Element, owner: str) -> list[dict] | None:
        """The fields of a type, as NVRE writes them."""
        names = self.names_of(elem)
        out = []
        for f in elem.findall("field"):
            conv = self.convert_field(f, owner, names)
            if conv is None:
                return None
            out.append(conv)
        return out

    def applies(self, elem: ET.Element) -> bool:
        """Whether a type is one this game has."""
        versions = elem.get("versions")
        if not versions:
            return True
        sets = {
            "#FO3#": {"FO3"},
            "#SKY#": {"SKY"},
            "#SSE#": {"SSE"},
            "#FO4#": {"FO4"},
            "#F76#": {"F76"},
            "#BETHESDA#": {"FO3", "SKY", "SSE", "FO4", "F76"},
            "#FO3_AND_LATER#": {"FO3", "SKY", "SSE", "FO4", "F76"},
            "#SKY_AND_LATER#": {"SKY", "SSE", "FO4", "F76"},
        }
        for token in versions.split():
            if token in sets:
                if self.game in sets[token]:
                    return True
            elif token == f"#{self.game}#":
                return True
        return False

    def convert_block(self, name: str) -> dict | None:
        """One block type, as NVRE writes it."""
        elem = self.objects.get(name)
        if elem is None or not self.applies(elem):
            return None
        fields = self.fields_of(elem, name)
        if fields is None:
            return None
        out: dict = {}
        if elem.get("abstract") == "true":
            out["abstract"] = True
        base = elem.get("inherit")
        if base:
            out["inherit"] = type_name(base)
        out["fields"] = fields
        return out

    def convert_struct(self, name: str, force: bool = False) -> dict | None:
        """One struct type, as NVRE writes it.

        `force` keeps a struct that the game itself does not have but that
        a block the game does have refers to: the reference is what
        matters, because a field of an unknown type is an error.
        """
        elem = self.structs.get(name)
        if elem is None:
            return None
        if not force and not self.applies(elem):
            return None
        fields = self.fields_of(elem, name)
        if fields is None:
            return None
        out: dict = {}
        base = elem.get("inherit")
        if base:
            out["inherit"] = type_name(base)
        if elem.get("generic") == "true":
            out["template"] = "T"
        out["fields"] = fields
        return out

    def convert_enum(self, name: str) -> dict | None:
        """One enum, as NVRE writes it."""
        elem = self.enums.get(name)
        if elem is None:
            return None
        storage = elem.get("storage", "uint")
        values = {}
        for o in elem.findall("option"):
            v = o.get("value")
            n = o.get("name")
            if v is None or n is None:
                continue
            try:
                values[str(int(v, 0))] = n
            except ValueError:
                continue
        return {"storage": PRIMS.get(storage, "u32"), "values": values}

    def convert_bitflags(self, name: str) -> dict | None:
        """One set of bit flags, as NVRE writes it."""
        elem = self.bitflags.get(name)
        if elem is None:
            return None
        storage = elem.get("storage", "uint")
        bits = {}
        for o in elem.findall("option"):
            b = o.get("bit")
            n = o.get("name")
            if b is None or n is None:
                continue
            try:
                bit = int(b, 0)
            except ValueError:
                continue
            bits[str(bit)] = n
        return {"storage": PRIMS.get(storage, "u32"), "bits": bits}

    def convert_bitfield(self, name: str) -> dict | None:
        """One set of bit fields, as NVRE writes it."""
        elem = self.bitfields.get(name)
        if elem is None:
            return None
        storage = elem.get("storage", "uint")
        bits = {}
        for m in elem.findall("member"):
            pos = m.get("pos")
            width = m.get("width")
            n = m.get("name")
            if pos is None or width is None or n is None:
                continue
            first = int(pos)
            last = first + int(width) - 1
            bits[f"{first}-{last}" if last != first else str(first)] = n
        return {"storage": PRIMS.get(storage, "u32"), "bits": bits}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("xml", help="the nif.xml to read")
    ap.add_argument("out", help="the directory to write the schema into")
    ap.add_argument("--game", default="FO4", choices=sorted(GAME_BS))
    ap.add_argument("--only", nargs="*", default=None)
    ap.add_argument("--blocks", action="store_true")
    ap.add_argument("--structs", action="store_true")
    ap.add_argument("--enums", action="store_true")
    ap.add_argument("--report-skipped", action="store_true")
    args = ap.parse_args()

    conv = Converter(args.xml, args.game)
    blocks: dict = {}
    structs: dict = {}
    enums: dict = {}
    if args.only:
        names = list(args.only)
    else:
        names = sorted(set(conv.objects) | set(conv.structs))
    for name in names:
        if args.blocks or not (args.structs or args.enums):
            b = conv.convert_block(name)
            if b is not None:
                blocks[type_name(name)] = b
        if args.structs or not (args.blocks or args.enums):
            s = conv.convert_struct(name)
            if s is not None:
                structs[type_name(name)] = s
    # A struct the game does not have but that one of its blocks or
    # structs refers to is kept anyway: the reference is what matters,
    # because a field of an unknown type is an error.
    #
    # The structs a block refers to are gathered first, then the ones
    # those structs refer to, and so on.
    def refs(fields: list[dict]) -> list[str]:
        out = []
        for f in fields:
            t = f.get("type")
            if isinstance(t, str) and t in conv.structs and t not in out:
                out.append(t)
        return out

    pending: list[str] = []
    for b in blocks.values():
        pending.extend(refs(b.get("fields", [])))
    for s in structs.values():
        pending.extend(refs(s.get("fields", [])))
    while pending:
        t = pending.pop()
        if t in structs:
            continue
        s = conv.convert_struct(t, force=True)
        if s is None:
            continue
        structs[t] = s
        pending.extend(refs(s.get("fields", [])))
    if args.enums or not (args.blocks or args.structs):
        for name in conv.enums:
            c = conv.convert_enum(name)
            if c is not None:
                enums[type_name(name)] = c
        for name in conv.bitflags:
            c = conv.convert_bitflags(name)
            if c is not None:
                enums[type_name(name)] = c
        for name in conv.bitfields:
            c = conv.convert_bitfield(name)
            if c is not None:
                enums[type_name(name)] = c

    os.makedirs(args.out, exist_ok=True)
    doc = {"format": "nvre.nif.schema/1"}
    if enums:
        doc["enums"] = enums
    if structs:
        doc["structs"] = structs
    if blocks:
        doc["blocks"] = blocks
    path = os.path.join(args.out, f"{args.game.lower()}.json")
    with open(path, "w") as fh:
        json.dump(doc, fh, indent=1, sort_keys=True)
        fh.write("\n")
    print(
        f"wrote {path}: {len(blocks)} blocks, {len(structs)} structs, "
        f"{len(enums)} enums, {len(conv.skipped)} skipped fields"
    )
    if args.report_skipped:
        for owner, why in conv.skipped:
            print(f"  {owner}: {why}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

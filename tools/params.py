#!/usr/bin/env python3
"""Harvest method signatures INCLUDING their default argument values.

`src/returns.rs` was harvested from `.pex` locals tables, which give the RETURN
type of a call but say nothing about arguments. `tools/params.py` then harvested
argument types from the shipped `.psc`. Neither is enough, because the compiler
also **appends a default value for every omitted trailing argument**:

    source:  BlockActivation()
    .pex:    callmethod blockActivation self ::NoneVar true

    source:  Activate(akActionRef)
    .pex:    callmethod activate ::temp0 ::temp1 actronaut false

    source:  GetLinkedRef()
    .pex:    callmethod getLinkedRef self ::temp0 None

So a signature needs three things per parameter: its type, whether it has a
default, and what that default is.

This writes `crates/nvre-papyrus/src/signatures.rs`.

Usage:
    python3 tools/params.py <Scripts.zip> > /tmp/sig.rs
"""

import re
import sys
import zipfile
from collections import defaultdict, Counter

# re.M matters: `^` otherwise matches only at the start of the file and every
# declaration is missed. re.S lets a parameter list wrap across lines, which the
# engine's own scripts do for long signatures.
# `Quest Property CurrFight Auto` - the type precedes the `Property` keyword.
PROP = re.compile(
    r"^\s*([A-Za-z_][\w\[\]]*)\s+Property\s+(\w+)",
    re.I | re.M,
)

# `[ \t]` and not `\s`, and no `re.S` on the anchor: with `\s` a newline
# satisfies the separator, so the optional return-type prefix reaches across a
# line boundary and swallows the previous declaration's `endFunction` as the
# next function's return type. That produced 926 entries like
# ("abortresurrection", "end") — a void function typed as returning `end`, which
# made the emitter allocate a temporary where the game writes `::NoneVar`.
# The parameter group keeps `re.S` so a signature may wrap lines.
CALL = re.compile(
    r"^[ \t]*(?:[A-Za-z_][\w\[\]]*[ \t]+)?(Function|Event)[ \t]+(\w+)[ \t]*\((.*?)\)",
    re.I | re.M | re.S,
)


def split_params(text):
    """`(Actor akA, float afV = 1.0)` -> ['Actor akA', 'float afV = 1.0']."""
    text = " ".join(text.split())
    if not text:
        return []
    out, depth, cur = [], 0, ""
    for ch in text:
        if ch in "([":
            depth += 1
        elif ch in ")]":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


def default_of(spec):
    """The default literal of a parameter, normalised, or None.

    Returns ('bool', 'true'), ('int', '0'), ('float', '0.0'), ('none', 'None')
    or ('str', 'foo'). Anything unparseable is None, meaning "no default we can
    reproduce" — the emitter then leaves the call short rather than inventing a
    value.
    """
    if "=" not in spec:
        return None
    _, _, raw = spec.partition("=")
    raw = raw.strip()
    if not raw:
        return None
    low = raw.lower()
    if low in ("true", "false"):
        return ("bool", low)
    if low == "none":
        return ("none", "None")
    if re.fullmatch(r"-?\d+", raw):
        return ("int", raw)
    if re.fullmatch(r"-?\d*\.\d+([eE][-+]?\d+)?", raw):
        return ("float", raw)
    m = re.fullmatch(r'"((?:[^"\\]|\\.)*)"', raw)
    if m:
        return ("str", m.group(1))
    return None


def parse_type_and_name(spec):
    """`Actor akA = None` -> ('actor', 'aka', ('none', 'None'))."""
    spec = spec.strip()
    if not spec:
        return None
    default = default_of(spec)
    head = re.split(r"\s*=", spec)[0].strip()
    if not head:
        return None
    bits = head.split()
    if len(bits) < 2:
        return None
    name = bits[-1]
    type_name = " ".join(bits[:-1])
    # `ObjectReference akA` or `float[2] af` or `int[] ai`.
    if not re.match(r"^[A-Za-z_]\w*(\[\d*\])?$", type_name):
        return None
    return type_name.lower(), name.lower(), default


def main():
    z = zipfile.ZipFile(sys.argv[1])

    arity = defaultdict(Counter)  # name -> Counter of full shapes
    static_arity = defaultdict(Counter)  # (class, name) -> Counter of shapes
    # (name, position) -> set of types seen there. A whole signature can be
    # ambiguous while a single position is not: `AddItem` is
    # `FormArray.AddItem(Form)` on one class and
    # `ObjectReference.AddItem(Form, int, bool)` on another, so the name is
    # dropped from the exact table — but argument 0 is `Form` in both, and that
    # is the one that decides whether the caller needs a `cast`.
    pos_types = defaultdict(lambda: defaultdict(set))
    # Return types, keyed the same two ways. A `.psc` declares
    # `Actor Function F()` or `Function F()` — the latter returns None.
    ret_of = defaultdict(Counter)
    static_ret = defaultdict(Counter)
    # Properties declared by each class: `Quest Property CurrFight auto` on
    # ArenaScript means `aScript.CurrFight` has type Quest, which is what decides
    # whether reading it needs a cast.
    class_props = defaultdict(Counter)
    pos_defaults = defaultdict(lambda: defaultdict(set))
    parents = {}  # class -> its `extends` parent
    total = 0

    for name in z.namelist():
        if not name.endswith(".psc"):
            continue
        try:
            src = z.read(name).decode("cp1252", "replace")
        except Exception:
            continue
        # The script's own name is its class name for static calls.
        cls = None
        m = re.search(r"^\s*scriptname\s+(\w+)(?:\s+extends\s+(\w+))?", src, re.I | re.M)
        if m:
            cls = m.group(1).lower()
            # The inheritance chain is what lets a call on an `Actor` find a
            # method declared on `ObjectReference`. Without it, every inherited
            # method looks unknown and gets neither an argument cast nor its
            # default padding.
            if m.group(2):
                parents[cls] = m.group(2).lower()

        # Every Function declaration, including those inside states.
        for pm in PROP.finditer(src):
            pname = pm.group(2).lower()
            ptype = pm.group(1).strip()
            if cls and re.match(r"^[A-Za-z_]\w*(\[\d*\])?$", ptype):
                class_props[(cls, pname)][ptype] += 1

        for mm in CALL.finditer(src):
            parsed = [parse_type_and_name(p) for p in split_params(mm.group(3))]
            if any(p is None for p in parsed):
                continue
            # A shape is the full parameter list: types AND defaults. Two
            # declarations with the same types but different defaults are
            # different signatures, because they pad an omitted call
            # differently.
            shape = tuple((t, d) for t, _n, d in parsed)
            fn = mm.group(2).lower()
            ret = (mm.group(0).split("Function")[0].split("function")[0]).strip()
            if mm.group(1).lower() == "event":
                ret = "None"
            elif not ret or not ret.split():
                ret = "None"
            elif not re.match(r"^[A-Za-z_]\w*(\[\d*\])?$", ret):
                # A return type is one identifier. Anything else means the
                # prefix matched something that is not a type.
                ret = "None"
            ret_of[fn][ret] += 1
            if cls:
                static_ret[(cls, fn)][ret] += 1
            for i, (t, d) in enumerate(shape):
                pos_types[fn][i].add(t)
                pos_defaults[fn][i].add(d)
            arity[fn][shape] += 1
            if cls:
                static_arity[(cls, fn)][shape] += 1
            total += 1

    print(f"  // {total} declarations", file=sys.stderr)
    # A signature is usable only when exactly one shape was observed. Report
    # ambiguity rather than picking the majority: a name can mean different
    # things on unrelated classes, and the majority shape mis-casts the rest.
    unambiguous = {k: v.most_common(1)[0] for k, v in arity.items() if len(v) == 1}
    print(f"  // {len(arity)} names, {len(unambiguous)} with one shape", file=sys.stderr)
    ambiguous = {k: v for k, v in arity.items() if len(v) > 1}
    print(f"  // {len(ambiguous)} ambiguous (dropped)", file=sys.stderr)
    # `unambiguous` values are (shape, count) pairs, so unpack the shape first.
    defaulted = sum(
        1
        for (shape, _n) in unambiguous.values()
        if any(d for _t, d in shape)
    )
    print(f"  // {defaulted} carry at least one defaulted parameter", file=sys.stderr)

    def emit(key, shape):
        types = ", ".join(f'"{t}"' for t, _d in shape)
        defaults = ", ".join(
            "None" if d is None else f'Some(("{d[0]}", "{d[1]}"))' for _t, d in shape
        )
        print(f'    ("{key}", &[{types}], &[{defaults}]),')

    print(f"  // {len(parents)} extends relationships", file=sys.stderr)
    print('/// `(class, parent)`, lowercased, from every `Scriptname X extends Y`.')
    print("pub const PARENTS: &[(&str, &str)] = &[")
    for c, p in sorted(parents.items()):
        print(f'    ("{c}", "{p}"),')
    print("];")
    print()
    unambiguous_pos = 0
    rows = []
    for fn, by_pos in sorted(pos_types.items()):
        width = max(by_pos) + 1
        slots = []
        for i in range(width):
            ts = by_pos.get(i)
            if not (ts and len(ts) == 1):
                slots.append(None)
                continue
            # The default is kept only when every declaration that reaches this
            # position agrees on it. A shorter declaration simply does not reach
            # the slot, so it is not counted as disagreeing.
            ts_defaults = pos_defaults[fn].get(i, set())
            d = next(iter(ts_defaults)) if len(ts_defaults) == 1 else None
            slots.append((next(iter(ts)), d))
        if any(s is not None for s in slots):
            unambiguous_pos += 1
            rows.append((fn, slots))
    print(f"  // {unambiguous_pos} names with a position-unambiguous type", file=sys.stderr)
    print()
    print("/// `(function, per-position type where every declaration agreed)`.")
    print("///")
    print("/// A position is kept even when the whole signature is ambiguous, because")
    print("/// the argument slot is what decides a `cast`: `additem` is")
    print("/// `(Form)` on FormArray and `(Form, int, bool)` on ObjectReference, so the")
    print("/// signature is dropped — but slot 0 is `Form` either way.")
    print("pub const ARG_POSITIONS: &[(&str, &[Option<(&str, Option<(&str, &str)>)>])] = &[")
    for fn, slots in rows:
        parts = []
        for s in slots:
            if s is None:
                parts.append("None")
                continue
            t, d = s
            dflt = "None" if d is None else f'Some(("{d[0]}", "{d[1]}"))'
            parts.append(f'Some(("{t}", {dflt}))')
        print(f'    ("{fn}", &[{", ".join(parts)}]),')
    print("];")
    print()
    # Return types: keep only names where every declaration agrees.
    ret_rows = []
    for fn, c in sorted(ret_of.items()):
        if len(c) == 1:
            ret_rows.append((fn, c.most_common(1)[0][0]))
    for (cls, fn), c in sorted(static_ret.items()):
        if len(c) == 1:
            ret_rows.append((f"{cls}.{fn}", c.most_common(1)[0][0]))
    print(f"  // {len(ret_rows)} return-type entries", file=sys.stderr)
    prop_rows = [(f"{c}.{p}", v.most_common(1)[0][0]) for (c, p), v in sorted(class_props.items()) if len(v) == 1]
    print(f"  // {len(prop_rows)} property types", file=sys.stderr)
    print()
    print("/// `(class.property, type)`, lowercased, from the corpus declarations.")
    print("///")
    print("/// `Quest Property CurrFight auto` on ArenaScript means `aScript.CurrFight`")
    print("/// has type Quest. Without it a property read through another object is")
    print("/// typed `var`, so no conversion is possible on the way into a Quest slot.")
    print("pub const PROP_TYPES: &[(&str, &str)] = &[")
    for k, v in prop_rows:
        print(f'    ("{k}", "{v}"),')
    print("];")
    print()
    print("/// `(function, return type)`, lowercased, from the corpus declarations.")
    print("///")
    print("/// A `.psc` spells the return type in front of `Function`: `Actor Function")
    print("/// GetCasterActor()` returns Actor, and a bare `Function Play(...)` returns")
    print("/// None. This is direct evidence, unlike deriving a call's result type from")
    print("/// the temporary it was stored into — that only covers calls that happen to")
    print("/// appear in a shipped script.")
    print("pub const RETURNS: &[(&str, &str)] = &[")
    for fn, r in ret_rows:
        print(f'    ("{fn}", "{r}"),')
    print("];")
    print()
    print(
        "/// `(function, parameter types, defaults)`, lowercased, measured from the corpus."
    )
    print("pub const SIGNATURES: &[(&str, &[&str], &[Option<(&str, &str)>])] = &[")
    for fn, (shape, _n) in sorted(unambiguous.items()):
        emit(fn, shape)
    # `static_arity` values are Counters of shapes, not the single (shape, count)
    # pair that `unambiguous` holds. Taking most_common(1) here is what makes a
    # class-qualified entry trustworthy: only names with exactly one observed
    # shape for that class are emitted.
    for (cls, fn), counter in sorted(static_arity.items()):
        if len(counter) == 1:
            emit(f"{cls}.{fn}", counter.most_common(1)[0][0])
    print("];")


if __name__ == "__main__":
    main()

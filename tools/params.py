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
CALL = re.compile(
    r"^\s*(?:[A-Za-z_][\w\[\]]*\s+)?(Function|Event)\s+(\w+)\s*\((.*?)\)",
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

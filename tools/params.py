#!/usr/bin/env python3
"""Harvest method signatures — argument types, not just return types.

`src/returns.rs` was harvested from `.pex` locals tables, which give the RETURN
type of a call (the temporary it stores into is declared) but say nothing about
the ARGUMENTS. Arguments are what decide whether the compiler inserts a `cast`
before a call, and the oracle showed that clearly:

    Self.MoveTo(oProp, fProp, fProp, iProp)
      -> ::oProp_var ::fProp_var ::fProp_var bare, but iProp CAST
    akA.MoveTo(oProp, afV, afV, afV)
      -> everything bare

`MoveTo` wants (ObjectReference, float, float, float, bool). `oProp` and `fProp`
already match; `iProp` is an int where a float is required, so it is cast. So the
rule is type-directed conversion, and it needs the parameter list.

Where the signatures come from: the shipped `.psc` files. A `.psc` declares its
own functions with real parameter types, and the shipped corpus includes the
engine's own scripts (`ObjectReference.psc`, `Actor.psc`, `Game.psc`, …), so most
engine signatures are readable directly rather than guessed.

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
CALL = re.compile(r"^\s*(?:[A-Za-z_][\w\[\]]*\s+)?(Function|Event)\s+(\w+)\s*\((.*?)\)", re.I | re.M | re.S)


def split_params(text):
    """`(Actor akA, float afV)` -> ['Actor akA', 'float afV']."""
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


def parse_type_and_name(spec):
    """`Actor akA` -> ('actor', 'aka'). Returns None when unparseable."""
    spec = spec.strip()
    if not spec:
        return None
    # Trailing modifiers and defaults are not part of the type.
    spec = re.split(r"\s*=", spec)[0]
    spec = spec.replace("[]", "[]")
    bits = spec.split()
    if len(bits) < 2:
        return None
    name = bits[-1]
    type_name = " ".join(bits[:-1])
    # `ObjectReference akA` or `float[2] af` or `int[] ai`.
    if not re.match(r"^[A-Za-z_]\w*(\[\d*\])?$", type_name):
        return None
    return type_name.lower(), name.lower()


def main():
    path = sys.argv[1]
    z = zipfile.ZipFile(path)

    # (method_lower) -> Counter of (argtypes tuple)
    arity = defaultdict(Counter)
    # (class_lower, method_lower) -> Counter of argtypes
    static_arity = defaultdict(Counter)
    total = 0

    for name in z.namelist():
        if not name.endswith(".psc"):
            continue
        try:
            raw = z.read(name)
        except Exception:
            continue
        src = raw.decode("cp1252", "replace")
        # The script's own name is its class name for static calls.
        cls = None
        m = re.search(r"^\s*scriptname\s+(\w+)", src, re.I | re.M)
        if m:
            cls = m.group(1).lower()

        # Every Function declaration, including those inside states.
        for mm in CALL.finditer(src):
            params = split_params(mm.group(3))
            parsed = [parse_type_and_name(p) for p in params]
            if any(p is None for p in parsed):
                continue
            types = tuple(t for t, _ in parsed)
            fn = mm.group(2).lower()
            arity[fn][types] += 1
            if cls:
                static_arity[(cls, fn)][types] += 1
            total += 1

    print(f"  // {total} declarations", file=sys.stderr)
    print(f"  // {len(arity)} distinct function names", file=sys.stderr)

    # A signature is usable when one argument list clearly dominates. Report
    # ambiguity rather than picking silently: the same method name can exist on
    # unrelated classes with different parameter types.
    unambiguous = {k: v.most_common(1)[0] for k, v in arity.items() if len(v) == 1}
    print(f"  // {len(unambiguous)} with a single observed signature", file=sys.stderr)
    ambiguous = {k: v for k, v in arity.items() if len(v) > 1}
    print(f"  // {len(ambiguous)} with more than one", file=sys.stderr)
    for k in list(ambiguous)[:8]:
        print(f"  //   {k}: {list(ambiguous[k].items())[:3]}", file=sys.stderr)

    rows = []
    for fn, (types, _n) in sorted(unambiguous.items()):
        rows.append((fn, types))
    # `static_arity` values are Counters of argtype tuples, not the single
    # (types, count) pair that `unambiguous` holds. Taking most_common(1) here
    # is what makes a class-qualified entry trustworthy: only names with exactly
    # one observed shape for that class are emitted.
    for (cls, fn), counter in sorted(static_arity.items()):
        if len(counter) == 1:
            rows.append((f"{cls}.{fn}", counter.most_common(1)[0][0]))

    for key, types in rows:
        joined = ", ".join(f'"{t}"' for t in types)
        print(f'    ("{key}", &[{joined}]),')


if __name__ == "__main__":
    main()

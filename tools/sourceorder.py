#!/usr/bin/env python3
"""Is the `.pex` function list in SOURCE DECLARATION order?

For years the working assumption in this crate was that the function and property
lists come out in a .NET hash-container iteration order that cannot be decoded
from the corpus. Reading Microsoft's reference source says otherwise:

  * `Dictionary<K,V>` enumerates `entries[]` ascending, i.e. INSERTION ORDER —
    it is not hash-ordered at all.
  * `Hashtable` enumerates buckets descending, and that order was tested against
    the corpus and reproduced the shipped order in 0.5% of scripts: noise.

So the remaining candidate is the simplest one: the lists are in the order the
compiler inserted them, which for functions is plausibly source order. This tests
that directly, and separately tests whether `GetState`/`GotoState` sit at the
front, where the emitter has always put them.
"""

import importlib.util
import re
import sys
import zipfile
from collections import Counter

spec = importlib.util.spec_from_file_location("i2", "tools/interning2.py")
i2 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(i2)

DECL = re.compile(r"^\s*(?:[A-Za-z_][\w\[\]]*\s+)?(Function|Event)\s+(\w+)", re.I | re.M)
STATE = re.compile(r"^\s*(?:auto\s+)?state\s+(\w+)", re.I | re.M)


def source_order(src):
    """Every Function/Event declaration, in file order, with its state.

    A state block runs from `state X` to `endState`; declarations inside it belong
    to that state, and declarations before the first one belong to the default
    state."""
    out = []
    state = ""
    for line in src.splitlines():
        m = STATE.match(line)
        if m:
            state = m.group(1)
            continue
        if re.match(r"^\s*endState", line, re.I):
            state = ""
            continue
        m = DECL.match(line)
        if m:
            out.append((state.lower(), m.group(2)))
    return out


def main():
    scripts_zip, bsa = sys.argv[1], sys.argv[2]
    z = zipfile.ZipFile(scripts_zip)
    pex = i2.bsa_files(bsa)

    srcs = {}
    for n in z.namelist():
        if n.lower().endswith(".psc"):
            leaf = n.rsplit("/", 1)[-1][:-4].lower()
            srcs[leaf] = z.read(n).decode("cp1252", "replace")

    # Whole-list comparisons.
    same_order = 0
    def_front = 0
    def_anywhere = 0
    considered = 0
    # First mismatch position, to see whether the problem is at the start or
    # spread through the list.
    first_bad = Counter()

    for name, blob in sorted(pex.items()):
        leaf = name.replace("\\", "/").rsplit("/", 1)[-1][:-4].lower()
        src = srcs.get(leaf)
        if src is None:
            continue
        try:
            hdr, S, uf, objs = i2.parse(blob, [])
        except Exception:
            continue
        if not objs:
            continue
        declared = source_order(src)
        if len(declared) < 4:
            continue

        # The shipped list, per state, as (state, name) pairs in file order.
        shipped = []
        for st in objs[0]["states"]:
            sname = S[st["name"]].lower()
            for fn, _f in st["funcs"]:
                shipped.append((sname, S[fn]))

        # Drop the injected pair and compare only the script's own functions,
        # since those are the ones a source order can predict.
        own_shipped = [x for x in shipped if x[1] not in ("GetState", "GotoState")]
        own_declared = [x for x in declared if x[1] not in ("GetState", "GotoState")]

        considered += 1
        if [n.lower() for _s, n in own_shipped] == [n.lower() for _s, n in own_declared]:
            same_order += 1
        else:
            for i, (a, b) in enumerate(zip(own_shipped, own_declared)):
                if a[1].lower() != b[1].lower():
                    first_bad[i] += 1
                    break

        # Where do the injected two sit?
        defstate = [S[f] for st in objs[0]["states"] if S[st["name"]] == "" for f, _ in st["funcs"]]
        if defstate[:2] == ["GetState", "GotoState"] or defstate[:2] == ["GotoState", "GetState"]:
            def_front += 1
        if "GetState" in defstate:
            def_anywhere += 1

    print(f"scripts considered (>=4 declarations): {considered}")
    print(f"  own functions in SOURCE DECLARATION order: {same_order} ({100.0*same_order/considered:.1f}%)")
    print(f"  injected pair first in the default state:  {def_front} ({100.0*def_front/considered:.1f}%)")
    print(f"  GetState somewhere in the default state:   {def_anywhere}")
    print()
    print("first mismatch position, for the scripts that differ:")
    for pos, n in sorted(first_bad.items())[:10]:
        print(f"  index {pos}: {n}")


if __name__ == "__main__":
    main()

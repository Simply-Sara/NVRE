#!/usr/bin/env python3
"""Which container order reproduces the shipped function list?

Established so far, all from Microsoft's MIT-licensed reference source plus the
corpus:

  * `String.GetHashCode` (.NET 2.0) is reproduced exactly — verified against the
    known value `"".GetHashCode() == 757602046`.
  * `Dictionary<K,V>` enumerates in INSERTION order.
  * `Hashtable` enumerates buckets DESCENDING, with open-addressing rehash.

Plain-hash `Hashtable` order reproduces the corpus in ~0.2% of scripts: noise.
Source order does too (2.6%). So this tries the remaining plausible shapes,
including the one the compiler's own behaviour hints at: the string table is
**case-folded**, so its container almost certainly uses a case-insensitive
comparer, which means the hash is taken of a case-normalised name.
"""

import importlib.util
import re
import sys
import zipfile

spec = importlib.util.spec_from_file_location("i2", "tools/interning2.py")
i2 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(i2)

HASH_PRIME = 101


def _s32(x):
    x &= 0xFFFFFFFF
    return x - 0x100000000 if x >= 0x80000000 else x


def _mix(h, w):
    return _s32(_s32(_s32(h << 5) + h + (h >> 27)) ^ w)


def net_hash(s):
    """String.GetHashCode, .NET Framework 2.0, WIN32 path."""
    h1 = (5381 << 16) + 5381
    h2 = h1
    u = [ord(c) for c in s]
    if len(u) % 2:
        u.append(0)
    u += [0, 0]
    i, n = 0, len(s)
    while n > 2:
        h1 = _mix(h1, u[2 * i] | (u[2 * i + 1] << 16))
        h2 = _mix(h2, u[2 * i + 2] | (u[2 * i + 3] << 16))
        i += 2
        n -= 4
    if n > 0:
        h1 = _mix(h1, u[2 * i] | (u[2 * i + 1] << 16))
    return _s32(h1 + _s32(h2 * 1566083941))


def ht(names, size, fold):
    slots = [None] * size
    for n in names:
        key = fold(n)
        seed = net_hash(key) & 0x7FFFFFFF
        incr = 1 + ((seed * HASH_PRIME) % (size - 1))
        b = seed % size
        for _ in range(size * 4):
            if slots[b] is None:
                slots[b] = n
                break
            b = (b + incr) % size
    return [n for n in reversed(slots) if n is not None]


PRIMES = [3, 7, 11, 17, 23, 29, 37, 47, 59, 71, 89, 107, 131, 163, 197, 239,
          293, 353, 431, 521, 631, 761, 919, 1103, 1327, 1597, 1931, 2333,
          2801, 3371, 4049, 4861, 5839, 7013, 8419, 10103, 12143, 14591,
          17519, 21023, 25229, 30293, 36353, 43627, 52361, 62851, 75431,
          90523, 108631, 130363, 156437]

FOLDS = {
    "identity": lambda s: s,
    "lower": lambda s: s.lower(),
    "upper": lambda s: s.upper(),
}


def main():
    z = zipfile.ZipFile(sys.argv[1])
    pex = i2.bsa_files(sys.argv[2])
    srcs = {
        n.rsplit("/", 1)[-1][:-4].lower(): z.read(n).decode("cp1252", "replace")
        for n in z.namelist()
        if n.lower().endswith(".psc")
    }

    # name -> how many scripts that (fold, size) reproduced exactly
    score = {}
    considered = 0
    for name, blob in sorted(pex.items()):
        leaf = name.replace("\\", "/").rsplit("/", 1)[-1][:-4].lower()
        src = srcs.get(leaf)
        if src is None:
            continue
        try:
            _h, S, _u, objs = i2.parse(blob, [])
        except Exception:
            continue
        if not objs:
            continue
        shipped = [
            S[fn]
            for st in objs[0]["states"]
            for fn, _f in st["funcs"]
            if S[fn] not in ("GetState", "GotoState")
        ]
        if len(shipped) < 8:
            continue
        considered += 1
        for fl, fn in FOLDS.items():
            for size in PRIMES:
                if size < len(shipped):
                    continue
                if ht(shipped, size, fn) == shipped:
                    score[(fl, size)] = score.get((fl, size), 0) + 1

    print(f"scripts with >=8 functions: {considered}\n")
    if not score:
        print("NO (fold, size) combination reproduced any script's order.")
    for (fl, size), n in sorted(score.items(), key=lambda kv: -kv[1])[:15]:
        print(f"  fold={fl:9} size={size:6}: {n:5} scripts ({100.0*n/considered:.1f}%)")


if __name__ == "__main__":
    main()

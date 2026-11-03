#!/usr/bin/env python3
"""Reproduce the .NET string hash and test the container ordering.

Everything here comes from Microsoft's published, MIT-licensed reference source
(`microsoft/referencesource`), not from any Bethesda binary:

  mscorlib/system/string.cs                 -> String.GetHashCode
  mscorlib/system/collections/hashtable.cs  -> bucket assignment + enumerator
  mscorlib/system/collections/generic/dictionary.cs -> insertion-order enumerator

Why this matters: the Papyrus compiler is a .NET Framework 2.0 app (CLR
v2.0.50727), and the order it writes the string table and the record lists in is
a property of Microsoft's containers, not of anything Bethesda wrote. .NET 2.0
predates randomized string hashing, so that order is stable and reproducible.

Two candidate orders:

  Dictionary<K,V>  enumerates entries[] ascending  -> INSERTION ORDER
  Hashtable        enumerates buckets DESCENDING   -> reverse bucket order

This script computes the hash, then asks which container explains the order the
shipped `.pex` files actually use.
"""

import struct
import sys
import zipfile
from collections import Counter

# ---------------------------------------------------------------------------
# String.GetHashCode, .NET Framework 2.0, WIN32 path.
#
#     int hash1 = (5381<<16) + 5381;
#     int hash2 = hash1;
#     int* pint = (int *)src;              // two UTF-16 units at a time
#     int len = this.Length;
#     while (len > 2) {
#         hash1 = ((hash1 << 5) + hash1 + (hash1 >> 27)) ^ pint[0];
#         hash2 = ((hash2 << 5) + hash2 + (hash2 >> 27)) ^ pint[1];
#         pint += 2; len -= 4;
#     }
#     if (len > 0) hash1 = ((hash1 << 5) + hash1 + (hash1 >> 27)) ^ pint[0];
#     return hash1 + (hash2 * 1566083941);
#
# All arithmetic is 32-bit and the `>> 27` is an ARITHMETIC shift on a signed int.
# ---------------------------------------------------------------------------


def _s32(x):
    """Wrap to a signed 32-bit int, the way C# `int` arithmetic does."""
    x &= 0xFFFFFFFF
    return x - 0x100000000 if x >= 0x80000000 else x


def _mix(h, word):
    # (h << 5) + h + (h >> 27), all on signed 32-bit, then xor with the word.
    return _s32(_s32(_s32(h << 5) + h + (h >> 27)) ^ word)


def net_string_hash(s):
    h1 = (5381 << 16) + 5381
    h2 = h1
    # The CLR string is null-terminated and reads whole 4-byte ints, so pad to
    # an even number of UTF-16 units; the terminator supplies the zeros.
    units = [ord(c) for c in s]
    if len(units) % 2:
        units.append(0)
    units += [0, 0]
    i = 0
    n = len(s)
    while n > 2:
        p0 = units[2 * i] | (units[2 * i + 1] << 16)
        p1 = units[2 * i + 2] | (units[2 * i + 3] << 16)
        h1 = _mix(h1, p0)
        h2 = _mix(h2, p1)
        i += 2
        n -= 4
    if n > 0:
        p0 = units[2 * i] | (units[2 * i + 1] << 16)
        h1 = _mix(h1, p0)
    return _s32(h1 + _s32(h2 * 1566083941))


# ---------------------------------------------------------------------------
# Hashtable bucket assignment and enumeration.
# ---------------------------------------------------------------------------
def hashtable_bucket(name, size):
    """`bucket = (GetHash(key) & 0x7FFFFFFF) % hashsize`."""
    return (net_string_hash(name) & 0x7FFFFFFF) % size


HASH_PRIME = 101


def ht_order(names, size):
    """The exact .NET Hashtable: OPEN ADDRESSING with rehashing, not chaining.

    From Microsoft's reference source:

        uint seed  = (uint)hashcode;                       // already & 0x7FFFFFFF
        uint incr  = (uint)(1 + ((seed * HashPrime) % ((uint)size - 1)));
        int b = (int)(seed % (uint)size);
        do {
            if (buckets[b].key == null) { place here; return; }
            b = (int)(((long)b + incr) % (uint)size);
        } while (true);

    and the enumerator walks bucket indices **descending**:

        while (bucket > 0) { bucket--; if (key != null) yield; }
    """
    slots = [None] * size
    for n in names:
        seed = net_string_hash(n) & 0x7FFFFFFF
        incr = 1 + ((seed * HASH_PRIME) % (size - 1))
        b = seed % size
        for _ in range(size * 4):
            if slots[b] is None:
                slots[b] = n
                break
            b = (b + incr) % size
    return [n for n in reversed(slots) if n is not None]


def main():
    zip_path, bsa_path = sys.argv[1], sys.argv[2]
    sys.path.insert(0, "tools")
    import importlib.util

    spec = importlib.util.spec_from_file_location("i2", "tools/interning2.py")
    i2 = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(i2)

    pex = i2.bsa_files(bsa_path)

    # Sanity: a hash of "" and a couple of short names, printed so a mismatch
    # against a known-good .NET value is obvious rather than silent.
    print("hash sanity:")
    for probe in ["", "a", "ab", "abc", "OnInit"]:
        print(f"  {probe!r:10} -> {net_string_hash(probe)}")

    # Which hashsize reproduces the shipped function order?
    hits = Counter()
    total = 0
    for name, blob in sorted(pex.items()):
        try:
            hdr, S, uf, objs = i2.parse(blob, [])
        except Exception:
            continue
        if not objs:
            continue
        fnames = []
        for st in objs[0]["states"]:
            for fn, _f in st["funcs"]:
                fnames.append(S[fn])
        # Injected names are not part of whatever container held the source
        # functions, so remove them before testing.
        fnames = [n for n in fnames if n not in ("GetState", "GotoState")]
        # Short lists match many candidate orders by coincidence, so only
        # scripts with enough functions to discriminate are informative.
        if len(fnames) < MIN_FUNCS:
            continue
        total += 1
        for size in PRIMES:
            if size < len(fnames):
                continue
            if ht_order(fnames, size) == fnames:
                hits[size] += 1

    print(f"\nscripts with >= {MIN_FUNCS} functions: {total}")
    print("hashsizes whose Hashtable order reproduces the shipped order:")
    if not hits:
        print("  none")
    for size, n in hits.most_common(12):
        print(f"  size {size:6}: {n:6} scripts  ({100.0*n/total:.1f}%)")


MIN_FUNCS = 8

# HashHelpers.primes, from Microsoft's reference source.
PRIMES = [
    3, 7, 11, 17, 23, 29, 37, 47, 59, 71, 89, 107, 131, 163, 197, 239, 293,
    353, 431, 521, 631, 761, 919, 1103, 1327, 1597, 1931, 2333, 2801, 3371,
    4049, 4861, 5839, 7013, 8419, 10103, 12143, 14591, 17519, 21023, 25229,
    30293, 36353, 43627, 52361, 62851, 75431, 90523, 108631, 130363, 156437,
    187751, 225307, 270371, 324449, 389357, 467237, 560689, 672827, 807403,
    968897, 1162687, 1395263, 1674319,
]

if __name__ == "__main__":
    main()

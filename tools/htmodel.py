#!/usr/bin/env python3
"""Model the .NET 2.0 Hashtable exactly and check it against the compiler.

Every rule below is transcribed from Microsoft's MIT-licensed reference source
(`microsoft/referencesource`), not from any Bethesda binary:

  mscorlib/system/string.cs                 -> String.GetHashCode
  mscorlib/system/collections/hashtable.cs  -> capacity, growth, probing, order

The interesting behaviour, and the reason a static "sort by hash" model fails:

  * A new Hashtable starts with **3 buckets** and `loadsize = (int)(3*0.72) = 2`.
  * Inserting when `count >= loadsize` calls `expand()`, which rehashes into
    `ExpandPrime(3) = 7` buckets.
  * **`rehash` walks the OLD bucket array in ASCENDING order** and re-inserts.
    So growth permutes the insertion order itself, which is why the emitted order
    changes shape at N=4 and again at N=10 and N=15.
  * The enumerator walks the final bucket array in **DESCENDING** order.

That cascade is what makes the list look arbitrary while being perfectly
deterministic — and it is why guessing a "sort key" could never work.

Observed from the compiler (functions F00..F(N-1) declared in order):

    N=3   00 01 02
    N=4   00 01 03 02
    N=10  09 00 05 04 01 08 02 07 03 06
    N=16  00 05 08 06 15 09 03 04 01 13 02 14 07 10 11 12
"""

import sys

PRIMES = [
    3, 7, 11, 17, 23, 29, 37, 47, 59, 71, 89, 107, 131, 163, 197, 239, 293,
    353, 431, 521, 631, 761, 919, 1103, 1327, 1597, 1931, 2333, 2801, 3371,
    4049, 4861, 5839, 7013, 8419, 10103, 12143, 14591, 17519, 21023, 25229,
    30293, 36353, 43627, 52361, 62851, 75431, 90523, 108631, 130363, 156437,
    187751, 225307, 270371, 324449, 389357, 467237, 560689, 672827, 807403,
    968897, 1162687, 1395263, 1674319,
]
LOAD_FACTOR = 0.72
HASH_PRIME = 101


def get_prime(minimum):
    for p in PRIMES:
        if p >= minimum:
            return p
    return PRIMES[-1]


def expand_prime(old_size):
    """`HashHelpers.ExpandPrime`: double, then the next prime >= that."""
    return get_prime(2 * old_size)


def _s32(x):
    x &= 0xFFFFFFFF
    return x - 0x100000000 if x >= 0x80000000 else x


def _mix(h, w):
    return _s32(_s32(_s32(h << 5) + h + (h >> 27)) ^ w)


def net_hash(s):
    """String.GetHashCode, .NET Framework 2.0, WIN32 path.

    Verified against the known value `"".GetHashCode() == 757602046`."""
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


class Hashtable:
    """Just enough of System.Collections.Hashtable to predict iteration order."""

    def __init__(self):
        # `new Hashtable()` -> Initialize(0) -> GetPrime(0) == 3 buckets.
        self.size = get_prime(0)
        self.slots = [None] * self.size
        self.count = 0
        self.loadsize = int(self.size * LOAD_FACTOR)

    def _put(self, slots, key):
        seed = net_hash(key) & 0x7FFFFFFF
        incr = 1 + ((seed * HASH_PRIME) % (len(slots) - 1))
        b = seed % len(slots)
        for _ in range(len(slots) * 4):
            if slots[b] is None:
                slots[b] = key
                return
            b = (b + incr) % len(slots)
        raise RuntimeError("table full")

    def _rehash(self, newsize):
        new = [None] * newsize
        # ASCENDING walk of the old array: this is what permutes insertion order.
        for old in self.slots:
            if old is not None:
                self._put(new, old)
        self.slots = new
        self.size = newsize
        self.loadsize = int(newsize * LOAD_FACTOR)

    def add(self, key):
        if self.count >= self.loadsize:
            self._rehash(expand_prime(self.size))
        self._put(self.slots, key)
        self.count += 1

    def order(self):
        """The enumerator: `while (bucket > 0) { bucket--; yield }`."""
        return [k for k in reversed(self.slots) if k is not None]


OBSERVED = {
    3: [0, 1, 2],
    4: [0, 1, 3, 2],
    5: [0, 4, 1, 3, 2],
    6: [0, 4, 5, 1, 3, 2],
    7: [0, 4, 1, 5, 2, 3, 6],
    8: [0, 4, 1, 7, 5, 2, 3, 6],
    9: [0, 4, 8, 1, 7, 5, 2, 3, 6],
    10: [9, 0, 5, 4, 1, 8, 2, 7, 3, 6],
    11: [9, 0, 5, 4, 1, 8, 2, 7, 10, 3, 6],
    12: [9, 0, 5, 4, 1, 8, 2, 3, 7, 10, 11, 6],
    13: [9, 0, 5, 4, 1, 8, 2, 12, 3, 7, 10, 11, 6],
    14: [9, 0, 5, 4, 1, 8, 13, 2, 12, 3, 7, 10, 11, 6],
    15: [0, 5, 8, 6, 9, 3, 4, 1, 13, 2, 14, 7, 10, 11, 12],
    16: [0, 5, 8, 6, 15, 9, 3, 4, 1, 13, 2, 14, 7, 10, 11, 12],
}


def predict(names, inject=("GetState", "GotoState")):
    t = Hashtable()
    for n in inject:
        t.add(n)
    for n in names:
        t.add(n)
    return [x for x in t.order() if x not in inject]


def main():
    print("N   want                                got                                 ok")
    good = 0
    for n in sorted(OBSERVED):
        names = [f"F{i:02d}" for i in range(n)]
        want = [names[i] for i in OBSERVED[n]]
        got = predict(names)
        ok = got == want
        good += ok
        print(f"{n:2}  {' '.join(want):35} {' '.join(got):35} {ok}")
    print(f"\n{good}/{len(OBSERVED)} reproduced exactly")


if __name__ == "__main__":
    main()

"""Harvest static return types from the shipped scripts.

Every call in a `.pex` writes its result into a temporary, and the enclosing
function's local table declares that temporary's type. So for every
`callstatic`, the destination temporary's declared type **is** the return type
the Creation Kit computed. Reading it out of the game's own files beats
transcribing a wiki: the wiki has no `::tempN` to check against.

Usage:
    python3 tools/rettypes.py <Data>/Skyrim - Misc.bsa

Prints the Rust table body for src/returns.rs.
"""
import sys, os, collections, importlib.util

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location(
    'interning2', os.path.join(HERE, 'interning2.py'))
i2 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(i2)


def harvest(bsa):
    pex = i2.bsa_files(bsa)
    types = collections.defaultdict(collections.Counter)
    ok = 0
    for leaf in sorted(pex):
        if not leaf.endswith('.pex'):
            continue
        trace = []
        try:
            _hdr, strings, _uf, _objs = i2.parse(pex[leaf], trace)
            ok += 1
        except Exception:
            continue
        for t in trace:
            if t[0] != 'calls':
                continue
            for op, argv, locals_ in t[1]:
                # callstatic is opcode 25; callmethod 23, callparent 24.
                if op != 25 or len(argv) < 3:
                    continue
                decl = dict(locals_)
                dest = argv[2]
                if dest[0] != 'ident' or dest[1] not in decl:
                    continue
                types[(strings[decl[dest[1]]].lower())] += 1
    return types, ok


def main():
    bsa = sys.argv[1]
    # The class and function are what the caller needs; that needs the raw
    # args, so redo the walk keeping them.
    pex = i2.bsa_files(bsa)
    table = collections.defaultdict(collections.Counter)
    for leaf in sorted(pex):
        if not leaf.endswith('.pex'):
            continue
        trace = []
        try:
            _h, strings, _u, _o = i2.parse(pex[leaf], trace)
        except Exception:
            continue
        for t in trace:
            if t[0] != 'calls':
                continue
            for op, argv, locals_ in t[1]:
                if op != 25 or len(argv) < 3:
                    continue
                decl = dict(locals_)
                dest = argv[2]
                if dest[0] != 'ident' or dest[1] not in decl:
                    continue
                if argv[0][0] != 'ident' or argv[1][0] != 'ident':
                    continue
                fn = strings[argv[0][1]]
                cls = strings[argv[1][1]]
                ty = strings[decl[dest[1]]]
                table[(cls.lower(), fn.lower())][ty] += 1
    rows = []
    for (cls, fn), counts in sorted(table.items()):
        if len(counts) != 1:
            continue          # ambiguous: more than one observed type
        rows.append((cls, fn, list(counts)[0]))
    print(f"// {len(rows)} unambiguous static returns, harvested from {bsa}")
    for cls, fn, ty in rows:
        print(f'    ("{cls}", "{fn}", "{ty}"),')


if __name__ == '__main__':
    main()

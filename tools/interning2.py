"""Decode the Creation Kit's string-table intern order.

The table is built while the compiler walks the script, and this reads the
game's own `.pex` files to work out that walk. It is the gate to byte parity:
every index in a `.pex` points into this table, so nothing downstream can match
until the order does.

The approach: parse a file while recording every string index *in the order the
reader meets it*, then ask whether that order equals the table's own. Where it
does not, the first index that breaks tells you which field the compiler
interned out of file order.

Usage:
    python3 tools/interning.py <Data>/Skyrim - Misc.bsa <Data>/Scripts.zip [N]
"""
import struct
import sys
import zipfile
import os
import re

MAGIC = 0xFA57C0DE


def load_ops():
    """The opcode shapes, read from the crate so the two cannot drift."""
    here = os.path.dirname(os.path.abspath(__file__))
    src = open(os.path.join(here, '..', 'crates/nvre-papyrus/src/opcodes.rs')).read()
    block = src.split('pub const SKYRIM')[1].split('];')[0]
    found = re.findall(
        r'name:\s*"([a-z_0-9]+)",\s*args:\s*(\d+),\s*varargs:\s*(true|false)', block)
    return [(int(a), v == 'true') for _n, a, v in found]


OPS = load_ops()


class R:
    def __init__(self, b):
        self.b, self.p = b, 0

    def u8(self):
        v = self.b[self.p]
        self.p += 1
        return v

    def u16(self):
        v = struct.unpack_from('>H', self.b, self.p)[0]
        self.p += 2
        return v

    def u32(self):
        v = struct.unpack_from('>I', self.b, self.p)[0]
        self.p += 4
        return v

    def u64(self):
        v = struct.unpack_from('>Q', self.b, self.p)[0]
        self.p += 8
        return v

    def s(self):
        n = self.u16()
        v = self.b[self.p:self.p + n]
        self.p += n
        return v


def parse(b, trace=None):
    """Parse a Skyrim `.pex`, appending (tag, index) to `trace` for every
    string index met, in the reader's order."""
    r = R(b)
    assert r.u32() == MAGIC
    hdr = dict(major=r.u8(), minor=r.u8(), game=r.u16(), ctime=r.u64(),
               src=r.s().decode('cp1252'), user=r.s().decode('cp1252'),
               mach=r.s().decode('cp1252'))
    n = r.u16()
    strings = [r.s().decode('cp1252') for _ in range(n)]

    def sid(tag):
        i = r.u16()
        if trace is not None:
            trace.append((tag, i))
        return i

    if r.u8():
        r.u64()
        for _ in range(r.u16()):
            sid('dbg.obj')
            sid('dbg.state')
            sid('dbg.func')
            r.u8()
            for _ in range(r.u16()):
                r.u16()
    user_flags = []
    for _ in range(r.u16()):
        user_flags.append((sid('userflag'), r.u8()))

    def value(tag):
        t = r.u8()
        if t == 0:
            return ('none', None)
        if t == 1:
            return ('ident', sid(tag))
        if t == 2:
            return ('str', sid(tag))
        if t == 3:
            v = struct.unpack_from('>i', r.b, r.p)[0]
            r.p += 4
            return ('int', v)
        if t == 4:
            r.p += 4
            return ('float', None)
        if t == 5:
            r.u8()
            return ('bool', None)
        raise ValueError(t)

    calls = []          # (opcode, [args]) for every call instruction met

    def func(tag):
        ret = sid(tag + '.ret')
        sid(tag + '.doc')
        r.u32()
        flags = r.u8()
        for _ in range(r.u16()):
            sid(tag + '.param')
            sid(tag + '.paramtype')
        locals_ = []
        for _ in range(r.u16()):
            locals_.append((sid(tag + '.local'), sid(tag + '.localtype')))
        for _ in range(r.u16()):
            op = r.u8()
            nargs, has_varargs = OPS[op]
            argv = [value(tag + '.arg') for _ in range(nargs)]
            if has_varargs:
                c = value(tag + '.arg')
                for _ in range(c[1]):
                    argv.append(value(tag + '.arg'))
            if op in (23, 24, 25):
                calls.append((op, argv, locals_))
        return dict(flags=flags, locals=locals_, ret=ret)

    objs = []
    for _ in range(r.u16()):
        o = dict(name=sid('obj.name'))
        r.u32()
        o['parent'] = sid('obj.parent')
        o['doc'] = sid('obj.doc')
        r.u32()
        o['autostate'] = sid('obj.autostate')
        o['vars'] = []
        for _ in range(r.u16()):
            vname = sid('var.name')
            vtype = sid('var.type')
            r.u32()  # the variable's user-flag bits; not a string
            o['vars'].append(dict(name=vname, type=vtype, init=value('var.init')))
        o['props'] = []
        for _ in range(r.u16()):
            p = dict(name=sid('prop.name'), type=sid('prop.type'),
                     doc=sid('prop.doc'))
            r.u32()
            fl = r.u8()
            p['flags'] = fl
            if fl & 4:
                p['autovar'] = sid('prop.autovar')
            else:
                if fl & 1:
                    p['get'] = func('prop.get')
                if fl & 2:
                    p['set'] = func('prop.set')
            o['props'].append(p)
        o['states'] = []
        for _ in range(r.u16()):
            st = dict(name=sid('state.name'))
            st['funcs'] = []
            for _ in range(r.u16()):
                st['funcs'].append((sid('state.func.name'), func('state.func')))
            o['states'].append(st)
        objs.append(o)
    if trace is not None:
        trace.append(('calls', calls))
    return hdr, strings, user_flags, objs


def bsa_files(path):
    """Every `.pex` in a BSA, as {leaf: bytes}."""
    d = open(path, 'rb').read()
    ver, fro, flags, nf, nfi, tfn, tff = struct.unpack_from('<IIIIIII', d, 4)
    rs = 24 if ver == 105 else 16
    p = fro + rs * nf
    folders = []
    for fi in range(nf):
        cnt = struct.unpack_from('<I', d, fro + rs * fi + 8)[0]
        ln = d[p]
        p += 1
        name = d[p:p + ln].rstrip(b'\0').decode('cp1252')
        p += ln
        recs = []
        for _ in range(cnt):
            fh, sf, fo = struct.unpack_from('<QII', d, p)
            p += 16
            recs.append((sf & 0x3FFFFFFF, fo, sf & 0x40000000))
        folders.append((name, recs))
    names = d[p:].split(b'\0')
    out, i = {}, 0
    for _name, recs in folders:
        for size, off, toggle in recs:
            nm = names[i].decode('cp1252')
            i += 1
            if not nm.lower().endswith('.pex'):
                continue
            blob = d[off:off + size]
            if bool(flags & 0x4) ^ bool(toggle):
                orig = struct.unpack_from('<I', blob, 0)[0]
                payload = blob[4:]
                if payload[:4] == b'\x04\x22\x4d\x18':
                    import lz4.frame
                    blob = lz4.frame.decompress(payload)
                else:
                    import zlib
                    blob = zlib.decompress(payload)
                assert len(blob) == orig, (nm, len(blob), orig)
            out[nm.lower()] = blob
    return out


def main():
    bsa, zip_path = sys.argv[1], sys.argv[2]
    limit = int(sys.argv[3]) if len(sys.argv) > 3 else 6
    pex = bsa_files(bsa)
    print(f"{len(pex)} scripts in the archive")
    zf = zipfile.ZipFile(zip_path)
    srcs = {n.rsplit('/', 1)[-1][:-4].lower(): n
            for n in zf.namelist() if n.lower().endswith('.psc')}

    shown = 0
    holds = 0
    tried = 0
    failed = {}
    for leaf in sorted(pex):
        # Archive keys carry their extension; the source map does not.
        bare = leaf[:-4] if leaf.endswith('.pex') else leaf
        if bare not in srcs:
            continue
        quiet = shown >= limit
        trace = []
        try:
            hdr, strings, uf, objs = parse(pex[leaf], trace)
        except Exception as e:
            failed[str(e)] = failed.get(str(e), 0) + 1
            continue
        tried += 1
        # The table's own order, versus the order the file mentions indices.
        first_mention = {}
        for pos, (tag, idx) in enumerate(trace):
            first_mention.setdefault(idx, (pos, tag))
        in_order = all(
            first_mention.get(k, (10 ** 9,))[0] < first_mention.get(k + 1, (10 ** 9,))[0]
            for k in range(len(strings) - 1) if k in first_mention and k + 1 in first_mention
        )
        if in_order:
            holds += 1
        if quiet:
            continue
        shown += 1
        print(f"\n=== {leaf}: {len(strings)} strings, "
              f"first-mention order {'HOLDS' if in_order else 'does NOT hold'}")
        for k, s in enumerate(strings[:30]):
            fm = first_mention.get(k)
            tag = fm[1] if fm else '-'
            pos = fm[0] if fm else -1
            print(f"  {k:3} {s!r:46} {tag:22} mention {pos}")
    print(f"\n{holds}/{tried} scripts where the table is in first-mention order")
    if failed:
        print("parse failures:")
        for k, v in sorted(failed.items(), key=lambda kv: -kv[1]):
            print(f"  {v:5}  {k}")


if __name__ == '__main__':
    main()

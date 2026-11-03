"""Discover the Creation Kit's string-table intern order from shipped .pex files.

Hypothesis, read off DLC2ThirskDoorScript:
  0  script name
  1  ""                    (every script's doc strings point here)
  2+ all function NAMES, in file order (states then default state)
  then script-level user-flag names
  then the parent class
  then per function, in file order: return type, doc, params, locals, body operands
"""
import struct
import sys
import zipfile
import os

MAGIC = 0xFA57C0DE
# The opcode arity table, read from the crate's own source so the two cannot
# drift: `opcodes.rs` is the one place the instruction shapes are written down.
def _load_ops():
    import re
    src = open(os.path.join(os.path.dirname(__file__), '..',
                            'crates/nvre-papyrus/src/opcodes.rs')).read()
    block = src.split('pub const SKYRIM')[1].split('];')[0]
    ops = re.findall(r'name:\s*"([a-z_0-9]+)",\s*args:\s*(\d+),\s*varargs:\s*(true|false)',
                     block)
    return [(int(a), v == 'true') for _n, a, v in ops]


OPS, VARS = _load_ops(), None


class R:
    def __init__(self, b):
        self.b, self.p = b, 0

    def u8(self):
        v = self.b[self.p]; self.p += 1; return v

    def u16(self):
        v = struct.unpack_from('>H', self.b, self.p)[0]; self.p += 2; return v

    def u32(self):
        v = struct.unpack_from('>I', self.b, self.p)[0]; self.p += 4; return v

    def u64(self):
        v = struct.unpack_from('>Q', self.b, self.p)[0]; self.p += 8; return v

    def f32(self):
        v = struct.unpack_from('>f', self.b, self.p)[0]; self.p += 4; return v

    def s(self):
        n = self.u16(); v = self.b[self.p:self.p + n]; self.p += n; return v


def parse(b):
    r = R(b)
    assert r.u32() == MAGIC
    hdr = dict(major=r.u8(), minor=r.u8(), game=r.u16(), ctime=r.u64(),
               src=r.s().decode('cp1252'), user=r.s().decode('cp1252'),
               mach=r.s().decode('cp1252'))
    n = r.u16()
    strings = [r.s().decode('cp1252') for _ in range(n)]

    def sid():
        return r.u16()

    if r.u8():
        r.u64()
        for _ in range(r.u16()):
            sid(); sid(); sid(); r.u8()
            for _ in range(r.u16()):
                r.u16()
    user_flags = []
    for _ in range(r.u16()):
        user_flags.append((sid(), r.u8()))

    def value():
        t = r.u8()
        if t == 0: return ('none', None)
        if t == 1: return ('ident', sid())
        if t == 2: return ('str', sid())
        if t == 3:
            # An int operand's value is needed: a vararg count is one, and it
            # decides how many operands follow.
            v = struct.unpack_from('>i', r.b, r.p)[0]
            r.p += 4
            return ('int', v)
        if t == 4:
            r.u32(); return ('float', None)
        if t == 5:
            r.u8(); return ('bool', None)
        raise ValueError(t)

    def func():
        ret = sid(); doc = sid(); r.u32(); flags = r.u8()
        params = [(sid(), sid()) for _ in range(r.u16())]
        locals_ = [(sid(), sid()) for _ in range(r.u16())]
        for _ in range(r.u16()):
            op = r.u8()
            nargs, has_varargs = OPS[op]
            for _ in range(nargs):
                value()
            if has_varargs:
                # The count is itself a tagged int value, then that many more.
                count = value()
                for _ in range(count[1]):
                    value()
        return dict(ret=ret, doc=doc, flags=flags, params=params, locals=locals_)

    objs = []
    for _ in range(r.u16()):
        o = dict(name=sid()); r.u32()
        o['parent'] = sid(); o['doc'] = sid(); r.u32(); o['autostate'] = sid()
        o['vars'] = [(sid(), sid(), r.u32(), value()) for _ in range(r.u16())]
        props = []
        for _ in range(r.u16()):
            p = dict(name=sid(), type=sid(), doc=sid()); r.u32(); fl = r.u8()
            p['flags'] = fl
            if fl & 4:
                p['autovar'] = sid()
            else:
                if fl & 1:
                    p['get'] = func()
                if fl & 2:
                    p['set'] = func()
            props.append(p)
        o['props'] = props
        states = []
        for _ in range(r.u16()):
            st = dict(name=sid())
            st['funcs'] = []
            for _ in range(r.u16()):
                st['funcs'].append((sid(), func()))
            states.append(st)
        o['states'] = states
        objs.append(o)
    return hdr, strings, user_flags, objs


def load(archive, want):
    bsa = archive[0]
    d = open(bsa, 'rb').read()
    ver, fro, flags, nf, nfi, tfn, tff = struct.unpack_from('<IIIIIII', d, 4)
    rs = 24 if ver == 105 else 16
    p = fro + rs * nf
    folders = []
    for fi in range(nf):
        cnt = struct.unpack_from('<I', d, fro + rs * fi + 8)[0]
        ln = d[p]; p += 1
        name = d[p:p + ln].rstrip(b'\0').decode('cp1252'); p += ln
        recs = []
        for _ in range(cnt):
            fh, sf, fo = struct.unpack_from('<QII', d, p); p += 16
            recs.append((sf & 0x3FFFFFFF, fo))
        folders.append((name, recs))
    names = d[p:].split(b'\0')
    i = 0
    # File names are stored relative to their folder, and the wanted path is
    # `scripts\<leaf>.pex`, so the leaf is what to match.
    leaf = want.rsplit('\\', 1)[-1].lower()
    for name, recs in folders:
        for size, off in recs:
            nm = names[i].decode('cp1252'); i += 1
            if name.lower() == 'scripts' and nm.lower() == leaf:
                blob = d[off:off + size]
                # Bit 0x4 of the archive flags means "compressed by default";
                # Bethesda's shipped archives leave it clear and store plain
                # bytes. A file whose own toggle bit is set is compressed
                # whatever the default says.
                compressed = bool(flags & 0x4)
                if ver == 105 and not (flags & 0x100):
                    # Embedded names mean the blob starts with its own path.
                    pass
                if not compressed:
                    return blob
                body = blob
                orig = struct.unpack_from('<I', body, 0)[0]
                payload = body[4:]
                if payload[:4] == b'\x04\x22\x4d\x18':
                    import lz4.frame
                    return lz4.frame.decompress(payload)
                import zlib
                return zlib.decompress(payload)
    return None


def walk(hdr, strings, user_flags, objs):
    """The prediction: a walk order that should equal `strings`.

    Measured, and it holds for every script looked at so far:

      0    the script's name
      1    "" (the empty string, which every doc string defaults to)
      2,3  GetState then GotoState, always, whether the source spells them or
           not — the compiler's own state-machine pair
      4+   the script's own function names, in file order
      then the script's user-flag names, in declaration order
      then the parent class
    """
    out = [hdr['src'].rsplit('.', 1)[0], '', 'GetState', 'GotoState']

    # The script's own function names, in file order.
    seen = set()
    for o in objs:
        for st in o['states']:
            for name, _f in st['funcs']:
                s = strings[name]
                if s in ('GetState', 'GotoState') or s in seen:
                    continue
                seen.add(s)
                out.append(s)
    for name, _bit in user_flags:
        out.append(strings[name])
    for o in objs:
        out.append(strings[o['parent']])
    return out


def main():
    bsa = sys.argv[1]
    zpath = sys.argv[2]
    zf = zipfile.ZipFile(zpath)
    names = sorted(n for n in zf.namelist() if n.lower().endswith('.psc'))
    hit = 0
    checked = 0
    for n in names[:400]:
        leaf = n.rsplit('/', 1)[-1]
        if ' ' in leaf:
            continue
        try:
            blob = load((bsa,), 'scripts\\' + leaf[:-4] + '.pex')
        except Exception:
            continue
        if blob is None:
            continue
        checked += 1
        try:
            hdr, strings, uf, objs = parse(blob)
        except Exception as e:
            print("parse fail", leaf, e)
            continue
        pred = walk(hdr, strings, uf, objs)
        # How far does the prediction agree with the real table?
        k = 0
        while k < len(pred) and k < len(strings) and pred[k] == strings[k]:
            k += 1
        if k == len(strings):
            hit += 1
        elif checked <= 6:
            print(f"{leaf}: {k}/{len(strings)} prefix, {len(pred)} predicted")
            print("  game:", strings[:14])
            print("  pred:", pred[:14])
    print(f"\n{hit}/{checked} scripts with a fully predicted table")


if __name__ == '__main__':
    main()

"""Reference parser for Skyrim PEX (.pex), used as ground truth for the Rust reader.

Written as one explicit statement per field so a mis-ordered read surfaces as a
wrong value instead of a shifted stream. Reads big-endian first, falls back to
little-endian (Fallout 4) if the magic is little-endian.

Usage: python3 tools/pexref.py <file.pex> [--summary] [--instrs]
"""
import struct
import sys
import os

MAGIC = 0xFA57C0DE

# Skyrim's opcode table: name, fixed argument count, takes a vararg list.
SKYRIM_OPS = [
    ("nop", 0, 0), ("iadd", 3, 0), ("fadd", 3, 0), ("isub", 3, 0), ("fsub", 3, 0),
    ("imul", 3, 0), ("fmul", 3, 0), ("idiv", 3, 0), ("fdiv", 3, 0), ("imod", 3, 0),
    ("not", 2, 0), ("ineg", 2, 0), ("fneg", 2, 0), ("assign", 2, 0), ("cast", 2, 0),
    ("cmp_eq", 3, 0), ("cmp_lt", 3, 0), ("cmp_lte", 3, 0), ("cmp_gt", 3, 0),
    ("cmp_gte", 3, 0), ("jmp", 1, 0), ("jmpt", 2, 0), ("jmpf", 2, 0),
    ("callmethod", 3, 1), ("callparent", 2, 1), ("callstatic", 3, 1),
    ("return", 1, 0), ("strcat", 3, 0), ("propget", 3, 0), ("propset", 3, 0),
    ("array_create", 2, 0), ("array_length", 2, 0), ("array_getelement", 3, 0),
    ("array_setelement", 3, 0), ("array_findelement", 4, 0),
    ("array_rfindelement", 4, 0),
]

PROP_READ = 1
PROP_WRITE = 2
PROP_AUTOVAR = 4


class Reader:
    def __init__(self, data, big_endian):
        self.d = data
        self.p = 0
        self.e = '>' if big_endian else '<'

    def need(self, n):
        if self.p + n > len(self.d):
            raise EOFError(f"want {n} bytes at {self.p}, file is {len(self.d)}")

    def u8(self):
        self.need(1)
        v = self.d[self.p]
        self.p += 1
        return v

    def u16(self):
        self.need(2)
        v = struct.unpack_from(self.e + 'H', self.d, self.p)[0]
        self.p += 2
        return v

    def u32(self):
        self.need(4)
        v = struct.unpack_from(self.e + 'I', self.d, self.p)[0]
        self.p += 4
        return v

    def i32(self):
        self.need(4)
        v = struct.unpack_from(self.e + 'i', self.d, self.p)[0]
        self.p += 4
        return v

    def u64(self):
        self.need(8)
        v = struct.unpack_from(self.e + 'Q', self.d, self.p)[0]
        self.p += 8
        return v

    def f32(self):
        self.need(4)
        v = struct.unpack_from(self.e + 'f', self.d, self.p)[0]
        self.p += 4
        return v

    def raw_string(self):
        n = self.u16()
        self.need(n)
        v = self.d[self.p:self.p + n]
        self.p += n
        return v


def parse(path, verbose=False):
    data = open(path, 'rb').read()
    if len(data) < 4:
        raise ValueError("too short")
    big = struct.unpack_from('>I', data, 0)[0] == MAGIC
    if not big and struct.unpack_from('<I', data, 0)[0] != MAGIC:
        raise ValueError("bad magic")
    r = Reader(data, big)
    r.u32()

    hdr = {}
    hdr['major'] = r.u8()
    hdr['minor'] = r.u8()
    hdr['game_id'] = r.u16()
    hdr['compilation_time'] = r.u64()
    hdr['source'] = r.raw_string().decode('cp1252')
    hdr['user'] = r.raw_string().decode('cp1252')
    hdr['machine'] = r.raw_string().decode('cp1252')

    n_str = r.u16()
    strings = []
    for _ in range(n_str):
        strings.append(r.raw_string().decode('cp1252'))

    def sid(i):
        if i >= len(strings):
            raise ValueError(f"string index {i} past the table's {len(strings)}")
        return strings[i]

    debug = None
    if r.u8():
        debug = {'modification_time': r.u64(), 'functions': []}
        n = r.u16()
        for _ in range(n):
            entry = {}
            entry['object'] = sid(r.u16())
            entry['state'] = sid(r.u16())
            entry['function'] = sid(r.u16())
            entry['type'] = r.u8()
            k = r.u16()
            entry['lines'] = [r.u16() for _ in range(k)]
            debug['functions'].append(entry)

    user_flags = []
    n = r.u16()
    for _ in range(n):
        name = sid(r.u16())
        bit = r.u8()
        user_flags.append((name, bit))

    def value():
        # The tag order is the compiler's own enum, measured on
        # DLC2ThirskDoorScript.pex's `return ::State` (01 00 12 = identifier):
        # 0 none, 1 identifier, 2 string, 3 int, 4 float, 5 bool. Reading 1 as
        # bool shifts the stream by one byte and every later index is garbage.
        t = r.u8()
        if t == 0:
            return ('none', None)
        if t == 1:
            return ('identifier', sid(r.u16()))
        if t == 2:
            return ('string', sid(r.u16()))
        if t == 3:
            return ('int', r.i32())
        if t == 4:
            return ('float', r.f32())
        if t == 5:
            return ('bool', r.u8() != 0)
        raise ValueError(f"value type {t} at {r.p - 1}")

    def function():
        f = {}
        f['return_type'] = sid(r.u16())
        f['doc'] = sid(r.u16())
        f['user_flags'] = r.u32()
        f['flags'] = r.u8()
        np = r.u16()
        f['params'] = [(sid(r.u16()), sid(r.u16())) for _ in range(np)]
        nl = r.u16()
        f['locals'] = [(sid(r.u16()), sid(r.u16())) for _ in range(nl)]
        ni = r.u16()
        ins = []
        for idx in range(ni):
            at = r.p
            op = r.u8()
            if op >= len(SKYRIM_OPS):
                raise ValueError(f"opcode {op} at {at}")
            name, nargs, varargs = SKYRIM_OPS[op]
            args = []
            for _ in range(nargs):
                args.append(value())
            extra = []
            if varargs:
                count = value()
                if count[0] != 'int':
                    raise ValueError(f"vararg count is {count} at {r.p}")
                for _ in range(count[1]):
                    extra.append(value())
            ins.append({'at': at, 'name': name, 'args': args, 'varargs': extra})
        f['instructions'] = ins
        return f

    objects = []
    n_obj = r.u16()
    for _ in range(n_obj):
        o = {}
        o['start'] = r.p
        o['name'] = sid(r.u16())
        o['size'] = r.u32()
        o['parent'] = sid(r.u16())
        o['doc'] = sid(r.u16())
        o['user_flags'] = r.u32()
        o['auto_state'] = sid(r.u16())

        nv = r.u16()
        o['variables'] = []
        for _ in range(nv):
            v = {}
            v['name'] = sid(r.u16())
            v['type'] = sid(r.u16())
            v['user_flags'] = r.u32()
            v['initial_value'] = value()
            o['variables'].append(v)

        np = r.u16()
        o['properties'] = []
        for _ in range(np):
            p = {}
            p['name'] = sid(r.u16())
            p['type'] = sid(r.u16())
            p['doc'] = sid(r.u16())
            p['user_flags'] = r.u32()
            p['flags'] = r.u8()
            if p['flags'] & PROP_AUTOVAR:
                p['auto_var'] = sid(r.u16())
            else:
                if p['flags'] & PROP_READ:
                    p['getter'] = function()
                if p['flags'] & PROP_WRITE:
                    p['setter'] = function()
            o['properties'].append(p)

        ns = r.u16()
        o['states'] = []
        for _ in range(ns):
            st = {}
            st['name'] = sid(r.u16())
            nf = r.u16()
            st['functions'] = []
            for _ in range(nf):
                fn_name = sid(r.u16())
                st['functions'].append((fn_name, function()))
            o['states'].append(st)

        o['end'] = r.p
        objects.append(o)

    return {
        'big_endian': big, 'header': hdr, 'strings': strings,
        'debug': debug, 'user_flags': user_flags, 'objects': objects,
        'consumed': r.p, 'total': len(data),
    }


def main():
    path = sys.argv[1]
    summary = '--summary' in sys.argv
    show_instrs = '--instrs' in sys.argv
    try:
        d = parse(path)
    except Exception as e:
        print(f"FAIL {path}: {e}")
        return 1
    h = d['header']
    print(f"{os.path.basename(path)}: v{h['major']}.{h['minor']} "
          f"game={h['game_id']} be={d['big_endian']} strings={len(d['strings'])} "
          f"objects={len(d['objects'])} consumed={d['consumed']}/{d['total']} "
          f"left={d['total'] - d['consumed']}")
    if summary:
        for o in d['objects']:
            ok = 'OK' if (o['end'] - o['start']) == o['size'] else 'MISMATCH'
            print(f"  {o['name']}: size={o['size']} actual={o['end'] - o['start']} {ok} "
                  f"vars={len(o['variables'])} props={len(o['properties'])} "
                  f"states={len(o['states'])}")
        return 0
    for o in d['objects']:
        ok = 'OK' if (o['end'] - o['start']) == o['size'] else 'MISMATCH'
        print(f"  object {o['name']} parent={o['parent']} size={o['size']} "
              f"actual={o['end'] - o['start']} {ok}")
        for v in o['variables']:
            print(f"    var {v['name']}:{v['type']} = {v['initial_value']}")
        for p in o['properties']:
            bits = ''.join(n for n, b in (('R', 1), ('W', 2), ('A', 4)) if p['flags'] & b)
            print(f"    prop {p['name']}:{p['type']} [{bits}]"
                  + (f" auto={p['auto_var']}" if 'auto_var' in p else ''))
        for st in o['states']:
            for name, f in st['functions']:
                print(f"    {st['name'] or '<empty>'}.{name} -> {f['return_type']} "
                      f"flags={f['flags']} params={[n for n, _ in f['params']]} "
                      f"locals={[n for n, _ in f['locals']]} "
                      f"instrs={len(f['instructions'])}")
                if show_instrs:
                    for i in f['instructions']:
                        a = ' '.join(f"{k}:{v}" for k, v in i['args'])
                        va = f" +{len(i['varargs'])}" if i['varargs'] else ''
                        print(f"        @{i['at']:5} {i['name']} {a}{va}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

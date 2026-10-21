#!/usr/bin/env python3
"""Which records have a different layout in two games' masters?

A per-subrecord size that differs between two games is a layout that
differs, and it is where a shared schema would go wrong. A size that
merely occurs a different number of times is not a difference in layout.

    tools/cmpgames.py "<master A>" "<master B>"
"""
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PROBE = os.path.join(ROOT, "target/debug/sizeprobe")
RECUSE = os.path.join(ROOT, "target/debug/recuse")


def sigs(path):
    out = subprocess.run([RECUSE, path], capture_output=True, text=True).stdout
    return sorted({line.split()[0] for line in out.splitlines() if line.split()})


def sizes(path, sig):
    out = subprocess.run([PROBE, path, sig], capture_output=True, text=True).stdout
    d = {}
    for line in out.splitlines():
        m = re.match(r"^  (\S+)\s+(.*)$", line)
        if m:
            # The sizes, not the counts: "24B x317" -> {24}.
            d[m.group(1)] = {int(x) for x in re.findall(r"(\d+)B", m.group(2))}
    return d


def main():
    a, b = sys.argv[1], sys.argv[2]
    for sig in sorted(set(sigs(a)) | set(sigs(b))):
        sa, sb = sizes(a, sig), sizes(b, sig)
        diffs = []
        for k in sorted(set(sa) | set(sb)):
            if sa.get(k, set()) != sb.get(k, set()):
                diffs.append(f"{k}: A={sorted(sa.get(k, []))} | B={sorted(sb.get(k, []))}")
        if diffs:
            print(sig)
            for d in diffs:
                print(f"    {d}")


if __name__ == "__main__":
    main()

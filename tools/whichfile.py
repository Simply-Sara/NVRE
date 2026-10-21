#!/usr/bin/env python3
"""Which file's definition of a record wins, and what does it hold?

A schema directory is read in path order, and a record in a later file
replaces the earlier one outright. So the last file that names a record is
the one that decides its fields — which is easy to get wrong when a record
is written in several places for different reasons.

    tools/whichfile.py LVLI REFR PACK
"""
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def load_dirs(dirs):
    """The files a schema is read from, in the order they are read."""
    files = []
    for d in dirs:
        for name in sorted(os.listdir(d)):
            path = os.path.join(d, name)
            if os.path.isfile(path) and name.endswith(".json"):
                files.append(path)
    return files


def main():
    dirs = sys.argv[1].split(",")
    wanted = sys.argv[2:]
    files = load_dirs([os.path.join(ROOT, d) for d in dirs])
    for sig in wanted:
        winner = None
        seen = []
        for f in files:
            try:
                j = json.load(open(f))
            except Exception:
                continue
            r = j.get("records", {}).get(sig)
            if r:
                seen.append((os.path.relpath(f, ROOT), sorted(r["subrecords"])))
                winner = (os.path.relpath(f, ROOT), sorted(r["subrecords"]))
        if not seen:
            print(f"{sig}: not defined")
            continue
        print(f"{sig}: wins from {seen[-1][0]}")
        for path, keys in seen:
            print(f"    {path}: {keys}")


if __name__ == "__main__":
    main()

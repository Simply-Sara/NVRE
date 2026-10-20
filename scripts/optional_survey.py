#!/usr/bin/env python3
"""List the optional subrecords of every record type, from the real masters.

A subrecord is optional when only some records of its type carry it. The
savers must not add such a subrecord to a record that did not have one.
"""
import json
import sys

with open("/tmp/survey_nv.json") as fh:
    survey = json.load(fh)

records = survey["records"]
by_type: dict[str, list[dict]] = {}
for pair in survey["pairs"]:
    by_type.setdefault(pair["record"], []).append(pair)

wanted = sys.argv[1:]
for rec, n in sorted(records.items()):
    if wanted and rec not in wanted:
        continue
    if n == 0:
        continue
    optional = []
    for pair in by_type.get(rec, []):
        c = pair["count"]
        if c < n:
            optional.append(f"{pair['sub']}={c}/{n}")
    if optional:
        print(f"{rec:5} n={n:<6} " + " ".join(optional))

#!/usr/bin/env python3
"""Report the record types the Objects window shows that the round-trip
probe does not cover yet."""

import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent

session = (ROOT / "crates/nvre-editor/src/session.rs").read_text()
i = session.find("NON_OBJECT_SIGS")
block = session[i:session.find("\n];", i)]
excluded = re.findall(r'b"([^"]+)"', block)

specs = sorted(p.stem for p in (ROOT / "target/tempDocs/Records").glob("*.md"))
in_scope = [t for t in specs if t not in excluded]

probe = (ROOT / "crates/nvre-editor/examples/roundtrip_all.rs").read_text()
covered = set(re.findall(r'\("([A-Z_0-9]+)",', probe))

survey = json.load(open("/tmp/survey_nv.json"))["records"]

print(f"{len(specs)} specs, {len(in_scope)} in the Objects window")
print(f"excluded: {', '.join(excluded)}")
print(f"the probe covers {len(covered)} types")
print()
print("not in the probe yet:")
for t in in_scope:
    if t not in covered:
        print(f"  {t:6} {survey.get(t, 0)}")

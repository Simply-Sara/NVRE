#!/usr/bin/env python3
"""Report the record types the Objects window shows that the round-trip
probe does not cover yet.

The type list comes from the specs in `target/tempDocs/Records` and the
excluded list from `NON_OBJECT_SIGS` in the session. The counts come from
the probe's own table, which is checked against the master when the probe
runs.
"""

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
covered = dict(re.findall(r'\("([A-Z_0-9]+)", b"[A-Z_0-9]+", (\d+)\)', probe))

print(f"{len(specs)} specs, {len(in_scope)} in the Objects window")
print(f"excluded: {', '.join(excluded)}")
print(f"the probe covers {len(covered)} types")

missing = [t for t in in_scope if t not in covered]
if missing:
    print()
    print("not in the probe yet:")
    for t in missing:
        print(f"  {t:6}")
else:
    print("every type in the Objects window is in the probe")

# A type in the probe that is not in the Objects window is worth knowing
# about too, since it means the probe is checking something the window
# never shows.
extra = [t for t in covered if t not in in_scope]
if extra:
    print()
    print("in the probe but not in the Objects window:")
    for t in extra:
        print(f"  {t:6} {covered[t]} records")

#!/usr/bin/env bash
# NVRE's full regression suite.
#
# Every number here has to stay where it is. The record schemas are measured
# against real game data, the NIF schemas against real meshes, and the
# round-trip check proves that what was decoded re-encodes to the same bytes.
#
# Usage: tools/checkall.sh [label]
set -u
cd "$(dirname "$0")/.." || exit 1

LABEL="${1:-check}"
FNV="/home/sara/.local/share/Steam/steamapps/common/Fallout New Vegas/Data"
FO3="/home/sara/Games/BethesdaGames/Fallout3/Data"
LE="/home/sara/Games/BethesdaGames/Skyrim/Data"
SE="/home/sara/Games/BethesdaGames/Skyrim Special Edition/Data"

say() { printf '\n=== %s ===\n' "$1"; }
fail=0
expect() { # expect <label> <needle> <text>
  if printf '%s' "$3" | grep -qF "$2"; then
    printf '  ok    %s\n' "$1"
  else
    printf '  FAIL  %s\n        wanted: %s\n        got: %s\n' "$1" "$2" "$3"
    fail=1
  fi
}

say "build"
out=$(cargo build --workspace --all-targets 2>&1)
expect "workspace builds" "Finished" "$out"
if printf '%s' "$out" | grep -q "^error"; then
  printf '%s\n' "$out" | grep "^error" -A 6 | head -40
  fail=1
fi

say "clippy"
out=$(cargo clippy --workspace --all-targets 2>&1)
expect "clippy is clean" "Finished" "$out"
if printf '%s' "$out" | grep -q "^warning"; then
  printf '%s\n' "$out" | grep "^warning" -A 6 | head -40
fi

say "unit tests"
for c in nvre-util nvre-inflate bsa105; do
  out=$(cargo test -p "$c" 2>&1)
  expect "$c tests" "test result: ok" "$out"
  printf '%s' "$out" | grep "test result:" | head -2
done

say "record schema coverage"
out=$(./target/debug/coverage --overlay schemas/fnv schemas/base "$FNV/FalloutNV.esm" 2>&1)
expect "FalloutNV.esm 100%" "total 2321456 ok 2321456 (100.00%) unknown 0 trailing 0 error 0" "$out"
out=$(./target/debug/coverage --overlay schemas/fnv --overlay schemas/fo3 schemas/base "$FO3/Fallout3.esm" 2>&1)
expect "Fallout3.esm 100%" "total 2634084 ok 2634084 (100.00%) unknown 0 trailing 0 error 0" "$out"
out=$(./target/debug/coverage --overlay schemas/skyrim --overlay schemas/skyrimle schemas/base "$LE/Skyrim.esm" 2>&1)
expect "Skyrim.esm (LE) 100%" "(100.00%) unknown 0 trailing 0 error 0" "$out"
out=$(./target/debug/coverage --overlay schemas/skyrim --overlay schemas/skyrimse schemas/base "$SE/Skyrim.esm" 2>&1)
expect "Skyrim.esm (SE) 100%" "(100.00%) unknown 0 trailing 0 error 0" "$out"

say "byte-identical round trip"
out=$(./target/debug/rtcheck --overlay schemas/fnv schemas/base "$FNV/FalloutNV.esm" 2>&1)
expect "FalloutNV.esm round trip" "ROUNDTRIP: all decoded subrecords re-encode byte-identically" "$out"
out=$(./target/debug/rtcheck --overlay schemas/skyrim --overlay schemas/skyrimse schemas/base "$SE/Skyrim.esm" 2>&1)
expect "Skyrim.esm (SE) round trip" "ROUNDTRIP: all decoded subrecords re-encode byte-identically" "$out"

say "NIF schema"
out=$(NIF_LIMIT=2000 ./target/debug/nifprobe "$FNV" "schemas/nif/base" 2>&1)
expect "FNV NIFs are perfect" "unknown 0, short 0, error 0, re-encode mismatches 0" "$out"
out=$(NIF_LIMIT=1200 ./target/debug/nifprobe "$LE" "schemas/nif/base:schemas/nif/skyrim:schemas/nif/skyrimle" "Skyrim - Meshes.bsa" 2>&1)
expect "Skyrim LE NIFs have no errors" "unknown 0" "$out"
printf '%s\n' "$out" | grep "blocks" | head -1

say "the editor's own asset path"
for g in fnv fo3 skyrim skyrimse; do
  out=$(./target/debug/examples/nifload "$g" 2>&1)
  expect "$g meshes parse" "parsed" "$out"
  printf '%s\n' "$out" | tail -1
done

say "meshes that build"
for g in fnv skyrim; do
  out=$(./target/debug/examples/nifmesh "$g" 2>&1)
  printf '%s\n' "$out" | tail -1
done

printf '\n=== %s: %s ===\n' "$LABEL" "$([ $fail -eq 0 ] && echo PASS || echo FAIL)"
exit $fail

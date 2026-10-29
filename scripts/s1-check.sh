#!/usr/bin/env bash
# Run the S1-1 re-encode check over every installed game, one process per
# game, writing each result to its own file so nothing is truncated.
#
# Each game takes its own schema chain — the base plus the family and version
# layers its GameProfile names — because a layer that does not apply makes the
# decode fail on the first block rather than on the last.
set -u
cd /home/sara/Projects/NVRE
OUT=/home/sara/.hermes/profiles/fallout-nv-modding/cache/scratch/s1
BIN=./target/release/examples/nifreencode

run() {
    local id="$1" data="$2" schema="$3"
    if [ ! -d "$data" ]; then
        echo "$id: not installed ($data)" >>"$OUT/missing.txt"
        return
    fi
    echo "=== $id ==="
    "$BIN" "$data" "$schema" >"$OUT/$id.txt" 2>&1
    grep -E '^(files |blocks |in |the archive)' "$OUT/$id.txt" | sed "s/^/  $id /"
}

mkdir -p "$OUT"
: >"$OUT/missing.txt"

run fnv     "/home/sara/.local/share/Steam/steamapps/common/Fallout New Vegas/Data" "schemas/nif/base"
run fo3     "/home/sara/Games/BethesdaGames/Fallout3/Data"                             "schemas/nif/base"
run skyrimle "/home/sara/Games/BethesdaGames/Skyrim/Data"                              "schemas/nif/base:schemas/nif/skyrim:schemas/nif/skyrimle"
run skyrimse "/home/sara/Games/BethesdaGames/Skyrim Special Edition/Data"              "schemas/nif/base:schemas/nif/skyrim:schemas/nif/skyrimse"
run fo4     "/home/sara/Games/BethesdaGames/Fallout4/Data"                             "schemas/nif/base:schemas/nif/fo4"

echo
echo "done; results in $OUT"

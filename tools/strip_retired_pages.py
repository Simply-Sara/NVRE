#!/usr/bin/env python3
"""Delete the object editor's retired pages and the state they read.

The pages are data now, so the 120 `draw_*` functions, the per-type `load_*` and
`save_*` methods, the 845-field `ObjectEditorState` and the registry that named
them are all unreachable. This removes them.

It is kept as a script rather than a one-off because the deletion has to be
repeatable: removing one item makes the next dead, so it runs passes until the
compiler stops naming anything.

Never cuts at a `;` chosen by `str.index`: a const's type can contain one
(`[(&str, &str); 10]`), which is exactly how the first attempt broke the file.
"""

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TARGET = REPO / "crates/nvre-editor/src/object_editor.rs"
NEW_IMPL = Path("/tmp/new_impl.rs")


def strip_item(src, name, kinds=("fn", "const", "static", "struct", "enum", "type")):
    """Remove the top-level item `name`, with its doc comment and attributes."""
    for kind in kinds:
        m = re.search(rf"^(?:pub )?{kind} {re.escape(name)}\b", src, re.M)
        if not m:
            continue
        head = m.start()
        lines = src[:head].split("\n")
        k = len(lines) - 1
        while k > 0 and lines[k - 1].lstrip().startswith(("///", "//", "#[")):
            k -= 1
        head = len("\n".join(lines[:k]))
        if head:
            head += 1
        if kind in ("const", "static", "type"):
            # To a `;` at bracket depth zero: a const's own type may carry one.
            depth = 0
            j = m.start()
            while j < len(src):
                c = src[j]
                if c in "([{<":
                    depth += 1
                elif c in ")]}>":
                    depth -= 1
                elif c == ";" and depth <= 0:
                    return src[:head] + src[j + 1:].lstrip("\n")
                j += 1
            return src
        j = src.index("{", m.start())
        depth = 0
        in_s = in_c = in_l = in_b = False
        while j < len(src):
            c = src[j]
            nxt = src[j + 1] if j + 1 < len(src) else ""
            if in_l:
                if c == "\n":
                    in_l = False
            elif in_b:
                if c == "*" and nxt == "/":
                    in_b = False
                    j += 1
            elif in_s:
                if c == "\\":
                    j += 1
                elif c == '"':
                    in_s = False
            elif in_c:
                if c == "\\":
                    j += 1
                elif c == "'":
                    in_c = False
            elif c == "/" and nxt == "/":
                in_l = True
                j += 1
            elif c == "/" and nxt == "*":
                in_b = True
                j += 1
            elif c == '"':
                in_s = True
            elif c == "'":
                in_c = True
            elif c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    return src[:head] + src[j + 1:].lstrip("\n")
            j += 1
    return src


def dead_items():
    """What the compiler says is unreachable."""
    out = subprocess.run(
        ["cargo", "check", "-p", "nvre-editor", "-j", "6"],
        cwd=REPO,
        capture_output=True,
        text=True,
        env={"CARGO_TARGET_DIR": "/home/sara/Projects/NVRE/target", "RUSTC_WRAPPER": "",
             "PATH": "/usr/bin:/bin:/home/sara/.cargo/bin"},
    )
    if out.returncode != 0:
        print(out.stderr[-3000:], file=sys.stderr)
        raise SystemExit("does not compile before the pass; refusing to edit")
    names = re.findall(
        r"warning: (?:constant|function|struct|method|enum|field|associated items?) "
        r"`([^`]+)` is never (?:used|read|constructed)",
        out.stderr,
    )
    return list(dict.fromkeys(names))


def main():
    src = TARGET.read_text()

    # 1. The header-only impl.
    start = next(i for i, l in enumerate(src.split("\n")) if l.startswith("impl ObjectEditorState {"))
    lines = src.split("\n")
    depth = 0
    end = len(lines) - 1
    for j in range(start, len(lines)):
        depth += lines[j].count("{") - lines[j].count("}")
        if depth == 0 and j > start:
            end = j
            break
    src = "\n".join(lines[:start]) + "\n" + NEW_IMPL.read_text().rstrip("\n") + "\n" + "\n".join(lines[end + 1:])

    # 2. Every hand-written page, and the registry that named them.
    for name in re.findall(r"^fn (draw_[a-z0-9_]+)\(", src, re.M):
        src = strip_item(src, name, kinds=("fn",))
    src = strip_item(src, "editor_block", kinds=("fn",))
    src = strip_item(src, "LegacyPage", kinds=("struct",))
    src = strip_item(src, "EditorBlock", kinds=("type",))
    src = re.sub(r"^impl LegacyPage \{[\s\S]*?^\}\n", "", src, flags=re.M)

    # 2b. The registry has two real pages now, so a third name is a schema typo.
    src = src.replace(
        """            let page: Option<Box<dyn EditorPage>> = match name {
                "grid" => Some(Box::new(GridPage::default())),
                "sections" => Some(Box::new(SectionPage::default())),
                _ => LegacyPage::for_name(name).map(|p| Box::new(p) as Box<dyn EditorPage>),
            };""",
        """            // Two pages, and a name that is neither is a schema typo. There used
            // to be a third source — a table of hand-written functions — and the
            // editor is data-driven now, so a name has to be one of these.
            let page: Option<Box<dyn EditorPage>> = match name {
                "grid" => Some(Box::new(GridPage::default())),
                "sections" => Some(Box::new(SectionPage::default())),
                _ => None,
            };""",
    )
    src = src.replace(
        """        name == "grid"
            || name == "sections"
            || self.map.contains_key(name)
            || editor_block(name).is_some()""",
        """        name == "grid" || name == "sections" || self.map.contains_key(name)""",
    )

    # 3. Passes until the compiler stops finding dead code.
    TARGET.write_text(src)
    for i in range(8):
        names = dead_items()
        if not names:
            print(f"pass {i}: nothing dead", file=sys.stderr)
            break
        for name in names:
            src = strip_item(src, name)
        TARGET.write_text(src)
        print(f"pass {i}: removed {len(names)}", file=sys.stderr)
    print(f"{TARGET}: {src.count(chr(10)) + 1} lines", file=sys.stderr)


if __name__ == "__main__":
    main()

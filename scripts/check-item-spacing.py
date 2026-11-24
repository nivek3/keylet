#!/usr/bin/env python3
"""Require a blank line between Rust items, which rustfmt cannot enforce on stable.

Flags an item's closing `}` (or a one-line item such as `fn a() {}`) followed directly by
another item (fn, impl, struct, ...) at the same indentation, including items that start
with attributes or doc comments.

Usage: check-item-spacing.py [--fix] [paths...]   (default path: crates)
"""

import pathlib
import re
import sys

ITEM = re.compile(
    r"(pub(\([^)]*\))?\s+)?((const|async|unsafe|extern(\s+\"[^\"]*\")?)\s+)*"
    r"(fn|impl|struct|enum|trait|mod|type|const|static|union|macro_rules!)\b"
)
PREFIX = re.compile(r"\s*(#\[|///|//!)")


def ends_item(line, indent):
    """A closing brace alone, or a whole one-line item such as `fn a() {}`."""
    body = line[len(indent) :]
    if body == "}":
        return True
    return bool(ITEM.match(body)) and body.endswith("}") and body.count("{") == body.count("}")


def violations(lines):
    """Yield the index of each line that ends an item and needs a blank line after it."""
    for i, line in enumerate(lines[:-1]):
        indent = line[: len(line) - len(line.lstrip())]
        if not ends_item(line.rstrip(), indent):
            continue
        following = lines[i + 1]
        if not following.startswith(indent) or following[len(indent) :].startswith((" ", "\t")):
            continue
        j = i + 1
        while j < len(lines) and PREFIX.match(lines[j]):
            # Attributes may span lines, as rustfmt writes long #[expect(...)] blocks.
            depth = lines[j].count("[") - lines[j].count("]")
            j += 1
            while depth > 0 and j < len(lines):
                depth += lines[j].count("[") - lines[j].count("]")
                j += 1
        if j < len(lines) and ITEM.match(lines[j][len(indent) :]):
            yield i


def main(args):
    fix = "--fix" in args
    roots = [pathlib.Path(arg) for arg in args if arg != "--fix"] or [pathlib.Path("crates")]
    files = sorted(f for root in roots for f in ([root] if root.is_file() else root.rglob("*.rs")))
    found = 0
    for path in files:
        text = path.read_text()
        lines = text.split("\n")
        hits = list(violations(lines))
        found += len(hits)
        if fix and hits:
            for i in reversed(hits):
                lines.insert(i + 1, "")
            path.write_text("\n".join(lines))
        elif not fix:
            for i in hits:
                print(f"{path}:{i + 2}: add a blank line before this item")
    if found and not fix:
        print(f"{found} item(s) need a blank line; run scripts/check-item-spacing.py --fix")
        return 1
    if fix:
        print(f"inserted {found} blank line(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

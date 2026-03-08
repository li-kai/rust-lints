#!/usr/bin/env python3
"""Scrape the Clippy lint website for MachineApplicable lints and emit structured output.

The Clippy team embeds applicability data in the rendered HTML at
https://rust-lang.github.io/rust-clippy/master/index.html — this is the only
machine-accessible source of that information. This script parses it and
produces JSON and TOML outputs grouped by lint category.

Usage:
    python3 scrape-clippy-applicability.py [--channel master|stable|nightly|rust-1.85.0]
"""

from __future__ import annotations

import argparse
import json
import sys
import urllib.request
from collections import defaultdict
from html.parser import HTMLParser


CLIPPY_URL = "https://rust-lang.github.io/rust-clippy/{channel}/index.html"


class LintParser(HTMLParser):
    """Extract (lint_id, group, applicability) triples from the Clippy lint index."""

    def __init__(self) -> None:
        super().__init__()
        self._in_article = False
        self._current_id: str | None = None
        self._current_group: str | None = None
        self._capture_applicability = False
        self._capture_group = False
        self.lints: list[tuple[str, str, str]] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attrs_dict = dict(attrs)
        if tag == "article":
            self._current_id = attrs_dict.get("id")
            self._current_group = None
            self._in_article = True
        if self._in_article and tag == "span":
            cls = attrs_dict.get("class", "") or ""
            if "applicability" in cls:
                self._capture_applicability = True
            elif "lint-group" in cls:
                self._capture_group = True

    def handle_data(self, data: str) -> None:
        if self._capture_group:
            self._current_group = data.strip()
            self._capture_group = False
        if self._capture_applicability:
            self.lints.append(
                (self._current_id or "", self._current_group or "", data.strip())
            )
            self._capture_applicability = False

    def handle_endtag(self, tag: str) -> None:
        if tag == "article":
            self._in_article = False


def fetch_html(channel: str) -> str:
    url = CLIPPY_URL.format(channel=channel)
    with urllib.request.urlopen(url, timeout=30) as resp:
        return resp.read().decode()


def parse_lints(html: str) -> list[tuple[str, str, str]]:
    parser = LintParser()
    parser.feed(html)
    return parser.lints


def build_output(lints: list[tuple[str, str, str]]) -> dict:
    by_group: dict[str, list[str]] = defaultdict(list)
    all_machine_applicable: list[str] = []

    for lint_id, group, applicability in lints:
        if applicability == "MachineApplicable":
            by_group[group].append(lint_id)
            all_machine_applicable.append(lint_id)

    return {
        "total_lints": len(lints),
        "machine_applicable_count": len(all_machine_applicable),
        "by_group": {k: sorted(v) for k, v in sorted(by_group.items())},
        "all": sorted(all_machine_applicable),
    }


def emit_json(data: dict) -> str:
    return json.dumps(data, indent=2)


def emit_cargo_toml_snippet(data: dict) -> str:
    """Emit a [lints.clippy] snippet that allows all MachineApplicable lints.

    Intended to be used in Cargo.toml so these lints stay silent during
    development, then re-enabled via -W flags in the pre-commit hook.
    """
    lines = [
        "# Auto-generated — do not edit manually.",
        "# These MachineApplicable lints are allowed during development",
        "# and auto-fixed at commit time by the pre-commit hook.",
        "#",
        "# Re-enable in pre-commit with:",
        "#   cargo clippy --fix --allow-dirty --allow-staged -- \\",
    ]
    all_lints = data["all"]
    for i, lint in enumerate(all_lints):
        sep = " \\" if i < len(all_lints) - 1 else ""
        lines.append(f"#     -W clippy::{lint}{sep}")
    lines.append("")

    for group, group_lints in data["by_group"].items():
        lines.append(f"# {group} ({len(group_lints)} lints)")
        for lint in group_lints:
            lines.append(f'{lint} = "allow"')
        lines.append("")

    return "\n".join(lines)


def emit_pre_commit_flags(data: dict) -> str:
    """Emit the -W flags for the pre-commit hook."""
    lines = [
        "#!/usr/bin/env bash",
        "# Auto-generated — do not edit manually.",
        "# Re-enable MachineApplicable lints and auto-fix them.",
        "",
        "cargo clippy --fix --allow-dirty --allow-staged -- \\",
    ]
    all_lints = data["all"]
    for i, lint in enumerate(all_lints):
        sep = " \\" if i < len(all_lints) - 1 else ""
        lines.append(f"  -W clippy::{lint}{sep}")
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--channel",
        default="stable",
        help="Clippy release channel or version to scrape (e.g. master, stable, nightly, rust-1.85.0)",
    )
    parser.add_argument(
        "--format",
        default="json",
        choices=["json", "cargo-toml", "pre-commit"],
        help="Output format (default: json)",
    )
    args = parser.parse_args()

    html = fetch_html(args.channel)
    lints = parse_lints(html)
    data = build_output(lints)

    if args.format == "json":
        print(emit_json(data))
    elif args.format == "cargo-toml":
        print(emit_cargo_toml_snippet(data))
    elif args.format == "pre-commit":
        print(emit_pre_commit_flags(data))


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Scrape the Clippy lint website for safe MachineApplicable lints and emit structured output.

The Clippy team embeds applicability data in the rendered HTML at
https://rust-lang.github.io/rust-clippy/master/index.html — this is the only
machine-accessible source of that information. The rendered lint articles also
include section headings such as "Known problems" / "Known issues", which this
script treats as an exclusion signal. It parses the HTML and produces JSON and
TOML outputs grouped by lint category.

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
KNOWN_PROBLEM_HEADINGS = {"known problems", "known issues"}


class LintParser(HTMLParser):
    """Extract lint metadata from the rendered Clippy lint index."""

    def __init__(self) -> None:
        super().__init__()
        self._in_article = False
        self._current_id: str | None = None
        self._current_group: str | None = None
        self._current_applicability: str | None = None
        self._current_has_known_problems = False
        self._capture_applicability = False
        self._capture_group = False
        self._capture_heading_tag: str | None = None
        self._heading_chunks: list[str] = []
        self.lints: list[tuple[str, str, str, bool]] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attrs_dict = dict(attrs)
        if tag == "article":
            self._current_id = attrs_dict.get("id")
            self._current_group = None
            self._current_applicability = None
            self._current_has_known_problems = False
            self._in_article = True
        if self._in_article and tag == "span":
            cls = attrs_dict.get("class", "") or ""
            if "applicability" in cls:
                self._capture_applicability = True
            elif "lint-group" in cls:
                self._capture_group = True
        if self._in_article and tag in {"h1", "h2", "h3", "h4", "h5", "h6"}:
            self._capture_heading_tag = tag
            self._heading_chunks = []

    def handle_data(self, data: str) -> None:
        text = data.strip()
        if self._capture_group:
            self._current_group = text
            self._capture_group = False
        if self._capture_applicability:
            self._current_applicability = text
            self._capture_applicability = False
        if self._capture_heading_tag:
            self._heading_chunks.append(data)

    def handle_endtag(self, tag: str) -> None:
        if self._capture_heading_tag == tag:
            heading = " ".join("".join(self._heading_chunks).split()).casefold()
            if heading in KNOWN_PROBLEM_HEADINGS:
                self._current_has_known_problems = True
            self._capture_heading_tag = None
            self._heading_chunks = []
        if tag == "article":
            if self._current_id and self._current_applicability:
                self.lints.append(
                    (
                        self._current_id,
                        self._current_group or "",
                        self._current_applicability,
                        self._current_has_known_problems,
                    )
                )
            self._in_article = False


def fetch_html(channel: str) -> str:
    url = CLIPPY_URL.format(channel=channel)
    with urllib.request.urlopen(url, timeout=30) as resp:
        return resp.read().decode()


def parse_lints(html: str) -> list[tuple[str, str, str, bool]]:
    parser = LintParser()
    parser.feed(html)
    return parser.lints


def build_output(lints: list[tuple[str, str, str, bool]]) -> dict:
    by_group: dict[str, list[str]] = defaultdict(list)
    all_machine_applicable: list[str] = []
    raw_machine_applicable: list[str] = []
    excluded_known_problem_lints: list[str] = []

    for lint_id, group, applicability, has_known_problems in lints:
        if applicability != "MachineApplicable":
            continue

        raw_machine_applicable.append(lint_id)
        if has_known_problems:
            excluded_known_problem_lints.append(lint_id)
            continue

        by_group[group].append(lint_id)
        all_machine_applicable.append(lint_id)

    return {
        "total_lints": len(lints),
        "raw_machine_applicable_count": len(raw_machine_applicable),
        "machine_applicable_count": len(all_machine_applicable),
        "excluded_known_problem_count": len(excluded_known_problem_lints),
        "excluded_known_problem_lints": sorted(excluded_known_problem_lints),
        "by_group": {k: sorted(v) for k, v in sorted(by_group.items())},
        "all": sorted(all_machine_applicable),
    }


def emit_json(data: dict) -> str:
    return json.dumps(data, indent=2)


def emit_cargo_toml_snippet(data: dict) -> str:
    """Emit a [lints.clippy] snippet that allows safe MachineApplicable lints.

    Intended to be used in Cargo.toml so these lints stay silent during
    development, then re-enabled via -W flags in the pre-commit hook.
    """
    lines = [
        "# Auto-generated — do not edit manually.",
        "# MachineApplicable lints without Known problems/issues:",
        "# silent during dev, auto-fixed at commit time.",
    ]

    for group, group_lints in data["by_group"].items():
        lines.append(f"# {group} ({len(group_lints)} lints)")
        for lint in group_lints:
            lines.append(f'{lint} = "allow"')
    # Remove trailing blank entry left by the last group
    if lines and lines[-1] == "":
        lines.pop()

    return "\n".join(lines)


def emit_pre_commit_flags(data: dict) -> str:
    """Emit a pre-commit hook that auto-fixes safe MachineApplicable lints."""
    lines = [
        "#!/usr/bin/env bash",
        "set -euo pipefail",
        "# Auto-generated — do not edit manually.",
        "# Only enable MachineApplicable lints with no Known problems/issues",
        "# detected in the rendered Clippy docs.",
        "",
    ]

    if not data["all"]:
        lines.extend(
            [
                "# No safe MachineApplicable lints were found for this channel.",
                "cargo clippy --fix --allow-dirty --allow-staged 2>/dev/null",
                "",
            ]
        )
        return "\n".join(lines)

    lines.append("cargo clippy --fix --allow-dirty --allow-staged -- \\")

    lint_lines = [
        f"  -W clippy::{lint} \\"
        for lint in data["all"][:-1]
    ]
    lines.extend(lint_lines)

    lines.append(f"  -W clippy::{data['all'][-1]} 2>/dev/null")

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

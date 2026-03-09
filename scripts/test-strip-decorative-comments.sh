#!/usr/bin/env bash
set -euo pipefail

AWK_SCRIPT="$(dirname "$0")/strip-decorative-comments.awk"
PASS=0
FAIL=0

assert_eq() {
    local name="$1" input="$2" expected="$3"
    actual="$(printf '%s\n' "$input" | awk -f "$AWK_SCRIPT")"
    if [ "$actual" = "$expected" ]; then
        printf '  ✓ %s\n' "$name"
        PASS=$((PASS + 1))
    else
        printf '  ✗ %s\n' "$name"
        printf '    expected: %s\n' "$(printf '%s' "$expected" | cat -A)"
        printf '    actual:   %s\n' "$(printf '%s' "$actual" | cat -A)"
        FAIL=$((FAIL + 1))
    fi
}

echo "Pure decoration lines → removed"

assert_eq "equals" \
    "// ================================" \
    ""

assert_eq "dashes" \
    "// --------------------------------" \
    ""

assert_eq "stars" \
    "// ********************************" \
    ""

assert_eq "tildes" \
    "// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~" \
    ""

assert_eq "underscores" \
    "// ________________________________" \
    ""

assert_eq "mixed decorative chars" \
    "// =-==-==-==-==-==-==-==-==-==-==" \
    ""

assert_eq "with leading whitespace" \
    "    // ================================" \
    ""

assert_eq "short decoration (3 chars)" \
    "// ---" \
    ""

echo ""
echo "Decoration with text → keep text, strip decoration"

assert_eq "equals with text" \
    "// ============ Configuration ============" \
    "// Configuration"

assert_eq "dashes with text" \
    "// --- Helper Functions ---" \
    "// Helper Functions"

assert_eq "leading decoration only" \
    "// ===== Public API" \
    "// Public API"

assert_eq "indented with text" \
    "    // ======== Tests ========" \
    " // Tests"

echo ""
echo "Kept as-is"

assert_eq "normal comment" \
    "// This is a normal comment" \
    "// This is a normal comment"

assert_eq "section header without decoration" \
    "// Configuration" \
    "// Configuration"

assert_eq "doc comment" \
    "/// Returns the configuration" \
    "/// Returns the configuration"

assert_eq "code line" \
    "let x = 42;" \
    "let x = 42;"

assert_eq "empty line" \
    "" \
    ""

assert_eq "short dashes in prose (2 chars, below threshold)" \
    "// use -- for flags" \
    "// use -- for flags"

assert_eq "url with dashes" \
    "// see https://example.com/foo-bar-baz" \
    "// see https://example.com/foo-bar-baz"

echo ""
echo "Multi-line input"

input="fn main() {
// ================================
// ======== Setup ========
    let x = 1;
// normal comment
// --------------------------------
    let y = 2;
}"

expected="fn main() {
// Setup
    let x = 1;
// normal comment
    let y = 2;
}"

assert_eq "mixed file preserves structure" "$input" "$expected"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]

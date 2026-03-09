# Strip decorative comment dividers from source files.
#
# Language-agnostic — works on any language using // comments
# (Rust, TypeScript, JavaScript, Go, C, etc.)
#
# Pure decoration lines (no words) are removed entirely.
# Lines with embedded text keep the text, lose the decoration.
#
# Usage: awk -f strip-decorative-comments.awk file.rs

/^[[:space:]]*\/\/[[:space:]]*[-=*~#_]{3,}[[:space:]]*$/ {
    # Pure decoration: // ========== or // --------- (no text)
    next
}

/^[[:space:]]*\/\/[[:space:]]*[-=*~#_]{3,}/ {
    # Decoration with embedded text: // ===== Config =====
    # Strip runs of decorative chars, collapse whitespace
    gsub(/[-=*~#_]{2,}/, "")
    gsub(/[[:space:]]+/, " ")
    gsub(/[[:space:]]+$/, "")
    print
    next
}

{ print }

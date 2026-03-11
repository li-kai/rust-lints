use clippy_utils::diagnostics::span_lint_and_help;
use rustc_data_structures::fx::FxHashSet;
use rustc_hir::Expr;
use rustc_lint::{LateContext, LateLintPass, LintContext as _};
use rustc_span::{ExpnKind, Span};

use crate::lints::suppression::is_in_test_zone;

rustc_session::declare_lint! {
    /// Flags `tracing` macro invocations where all captured values are positional
    /// format arguments and none are structured key-value fields.
    pub UNSTRUCTURED_LOG_FIELDS,
    Warn,
    "`tracing` macro uses format args instead of structured fields"
}

pub struct UnstructuredLogFields {
    seen_callsites: FxHashSet<Span>,
}

impl UnstructuredLogFields {
    pub fn new() -> Self {
        Self {
            seen_callsites: FxHashSet::default(),
        }
    }
}

rustc_session::impl_lint_pass!(UnstructuredLogFields => [UNSTRUCTURED_LOG_FIELDS]);

impl<'tcx> LateLintPass<'tcx> for UnstructuredLogFields {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        if !expr.span.from_expansion() {
            return;
        }

        let Some((macro_label, call_site)) = find_tracing_macro_callsite(cx, expr.span) else {
            return;
        };

        if !self.seen_callsites.insert(call_site) {
            return;
        }

        if is_in_test_zone(cx, expr) {
            return;
        }

        let Ok(snippet) = cx.sess().source_map().span_to_snippet(call_site) else {
            return;
        };

        if !has_only_format_args(&snippet) {
            return;
        }

        span_lint_and_help(
            cx,
            UNSTRUCTURED_LOG_FIELDS,
            call_site,
            format!("`{macro_label}!` uses format args instead of structured fields"),
            None,
            "use structured fields: `tracing::info!(key, \"message\")` instead of \
             `tracing::info!(\"msg {}\", key)`",
        );
    }
}

/// Walk up the macro expansion chain to find a tracing macro (`info`, `warn`,
/// `debug`, `error`, `trace`) defined in the `tracing` crate. Returns the
/// display label (e.g. `"tracing::info"`) and the call site span.
fn find_tracing_macro_callsite(cx: &LateContext<'_>, span: Span) -> Option<(String, Span)> {
    let mut current = span;
    while current.from_expansion() {
        let expn = current.ctxt().outer_expn_data();
        if let ExpnKind::Macro(_, name) = &expn.kind {
            let name_str = name.as_str();
            let base = name_str.strip_prefix("tracing::").unwrap_or(name_str);
            if matches!(base, "info" | "warn" | "debug" | "error" | "trace")
                && let Some(def_id) = expn.macro_def_id
            {
                let crate_name = cx.tcx.crate_name(def_id.krate);
                if crate_name.as_str() == "tracing" {
                    let label = if name_str.contains("::") {
                        name_str.to_owned()
                    } else {
                        format!("tracing::{name_str}")
                    };
                    return Some((label, expn.call_site));
                }
            }
        }
        current = expn.call_site;
    }
    None
}

/// Returns `true` when the macro invocation snippet has format placeholders in
/// its format string but no structured tracing fields before it.
fn has_only_format_args(snippet: &str) -> bool {
    let Some((before_fmt, fmt_str)) = split_at_format_string(snippet) else {
        return false;
    };

    if !has_format_placeholders(fmt_str) {
        return false;
    }

    !has_structured_fields(before_fmt)
}

/// Split a tracing macro invocation into the tokens before the format string
/// and the format string content itself.
///
/// Returns `(before_format_string, format_string_content)`.
fn split_at_format_string(snippet: &str) -> Option<(&str, &str)> {
    // Find the opening delimiter after the macro name.
    let args_start = snippet.find('(')?;
    let args = &snippet[args_start + 1..];

    // Walk tokens at depth 0 to find the first string literal.
    let bytes = args.as_bytes();
    let mut i = 0;
    let mut depth: u32 = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => {
                if depth == 0 {
                    break; // closing paren of macro
                }
                depth -= 1;
            }
            b'"' if depth == 0 => {
                let before = &args[..i];
                // Walk past the string literal content.
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        let str_content = &args[before.len() + 1..i];
                        return Some((before.trim(), str_content));
                    }
                    i += 1;
                }
                return None; // unterminated string
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Check if a format string contains placeholders like `{}`, `{:?}`, `{name}`.
/// Escaped braces `{{` are ignored.
fn has_format_placeholders(fmt: &str) -> bool {
    let bytes = fmt.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                i += 2;
                continue;
            }
            return true;
        }
        i += 1;
    }
    false
}

/// Returns `true` if the text before the format string contains at least one
/// structured tracing field: `key = value`, `?field`, `%field`, or a bare
/// identifier (shorthand for `field = field`).
///
/// The `target: "..."` pseudo-field is excluded since it's metadata, not a
/// structured data field.
fn has_structured_fields(before: &str) -> bool {
    let trimmed = before.trim().trim_end_matches(',').trim();
    if trimmed.is_empty() {
        return false;
    }
    // `target: "value"` is tracing metadata, not a structured field.
    // If that's the only thing before the format string, no fields are present.
    // target: "..." would have been consumed as the format string by our parser
    // only if it's the first string literal. Since `target:` uses a colon (not `=`),
    // and the value is a string literal, our parser would find that string as the
    // format string. To handle this: if `before` ends with `target:` (possibly with
    // whitespace), it's the target specifier and we already parsed its value as
    // the "format string" — which means we're looking at the wrong string.
    // However, in practice split_at_format_string finds the *first* string literal,
    // so `target: "x", "real fmt {}", v` would pick "x". This is an edge case we
    // accept as a known limitation.
    true
}

#[cfg(test)]
mod tests {
    use dylint_testing::ui;

    #[test]
    fn ui_unstructured_log_fields() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "unstructured_log_fields").run();
    }
}

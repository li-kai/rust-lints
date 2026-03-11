# `unstructured_log_fields`

**Level:** `warn`

Flags `tracing` macro invocations where all captured values are positional format arguments and none are structured key-value fields.

## Why

Tracing's power comes from structured fields — they enable filtering, indexing, and machine-readable logs. When every value is interpolated into a format string (`tracing::info!("user {} path {}", user_id, path)`), that structure is lost: the values are baked into an opaque string, invisible to subscribers and query tools.

- **Unqueryable** — `user_id=42` in a format string can't be filtered by a `tracing` subscriber or log aggregator without regex.
- **Inconsistent** — mixed structured/unstructured logging in a codebase makes it unclear which fields are available for querying.
- **Easy to fix** — `tracing::info!(user_id, path, "user hit endpoint")` is the same number of tokens and strictly more useful.

### Relation to Clippy

No Clippy lint covers this. Clippy doesn't inspect tracing macro semantics.

## Examples

### Triggers

```rust
fn handle_request() {
    let user_id = 42;
    let path = "/api/v1";
    tracing::info!("user {} hit {}", user_id, path);
    //~^ WARNING: `tracing::info!` uses format args instead of structured fields
}
```

```rust
fn process() {
    let count = 10;
    tracing::warn!("processed {} items", count);
    //~^ WARNING: `tracing::warn!` uses format args instead of structured fields
}
```

```rust
fn debug_request() {
    let req = "GET /";
    tracing::debug!("request: {:?}", req);
    //~^ WARNING: `tracing::debug!` uses format args instead of structured fields
}
```

### Does not trigger

```rust
// Structured fields (bare identifier shorthand)
tracing::info!(user_id, path, "user hit endpoint");

// Debug field shorthand
tracing::debug!(?req, "incoming request");

// Display field shorthand
tracing::info!(%addr, "listening");

// Explicit key-value
tracing::info!(user_id = user_id, "processing");

// Mixed structured + positional — at least one structured field present,
// partial improvement is acceptable
tracing::info!(user_id, "user performed {}", action);

// Bare message with no captures — nothing to structure
tracing::info!("server started");

// Non-tracing macros (log crate has no structured field support)
log::info!("user {}", user_id);

// Suppressed with #[allow]
#[allow(unstructured_log_fields)]
fn allowed() {
    tracing::info!("user {}", 42);
}
```

## Implementation notes

### Lint pass

`EarlyLintPass::check_mac` — inspect `MacCall` nodes before macro expansion. The macro's path identifies whether it's a tracing macro (`tracing::info`, `tracing::warn`, etc.), and the token stream contains the raw argument syntax with structured fields and format strings directly visible.

This avoids the complexity of `LateLintPass` where tracing macros have already expanded through multiple layers (`info!` → `$crate::event!` → internal HIR), making it difficult to recover the original argument structure.

### Detection algorithm

1. Match the macro path against known tracing macros: `info`, `warn`, `debug`, `error`, `trace` (with optional `tracing::` prefix).
2. Parse the token stream to find the format string literal (first string literal token at the top level).
3. Check for structured fields **before** the format string: `key = value`, `?field`, `%field`, or bare identifiers.
4. Check the format string for capture placeholders: `{}`, `{:?}`, `{name}`, etc.
5. Fire only when there are positional captures **and** no structured fields.

### Token stream structure

Tracing macro syntax (simplified):

```
tracing::info!( [fields,]* "format string" [, positional_args]* )
```

| Position | Syntax | Meaning |
|---|---|---|
| Before format string | `key = value` | Explicit key-value field |
| Before format string | `?field` | Debug-formatted field |
| Before format string | `%field` | Display-formatted field |
| Before format string | `bare_ident` | Shorthand for `bare_ident = bare_ident` |
| The format string | `"msg {}"` | Format string with placeholders |
| After format string | `, value` | Positional capture for `{}` placeholder |

### Skip conditions

| Condition | Reason |
|---|---|
| No format placeholders and no trailing args | Bare message — nothing to structure |
| At least one structured field present | Partial structuring is acceptable |
| Macro path doesn't match tracing macros | Not a tracing macro |
| `#[allow(unstructured_log_fields)]` | Explicitly suppressed |

### Diagnostic

```
warning: `tracing::info!` uses format args instead of structured fields
  --> src/handler.rs:15:5
   |
15 |     tracing::info!("user {} hit {}", user_id, path);
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: use structured fields: `tracing::info!(user_id, path, "message")` instead of `tracing::info!("user {} path {}", user_id, path)`
```

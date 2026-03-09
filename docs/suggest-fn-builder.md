# `suggest_fn_builder`

**Level:** `warn`

Suggests adding `#[bon::builder]` to functions with many parameters to enable named arguments at call sites.

## Why

Functions with many positional parameters are hard to call correctly:

- **Unreadable call sites** — `connect("localhost", 5432, 30, 3, true)` gives no indication of what each argument means.
- **Easy to swap arguments** — two adjacent parameters of the same type (e.g. `timeout` and `retries` are both numeric) can be silently transposed.
- **Painful to extend** — adding a parameter means updating every call site, even when a sensible default exists.

The `#[bon::builder]` attribute generates a builder with named setters, turning call sites into `connect().host("localhost").port(5432).tls(true).call()`.

### Relation to Clippy

Clippy has `too_many_arguments` (warn, default threshold 7) which flags the same symptom but only says "this function has too many arguments." It does not suggest a concrete fix. This lint has a lower default threshold and points directly at `bon::builder` as the solution.

## Examples

### Triggers

```rust
fn connect(host: &str, port: u16, timeout: u64, retries: u32, tls: bool) {
    //~^ WARNING: function `connect` has 5 parameters — consider adding `#[bon::builder]`
    // ...
}
```

```rust
impl Server {
    pub fn configure(addr: &str, port: u16, workers: usize, backlog: u32) {
        //~^ WARNING: function `configure` has 4 parameters — consider adding `#[bon::builder]`
        // ...
    }
}
```

### Does not trigger

```rust
// Already has #[bon::builder]
#[bon::builder]
fn connect(host: &str, port: u16, timeout: u64, retries: u32, tls: bool) {
    // ...
}

// Below threshold (default 4)
fn add(a: i32, b: i32, c: i32) -> i32 {
    a + b + c
}

// `self` parameter does not count
impl Server {
    fn configure(&self, addr: &str, port: u16, workers: usize) {
        // ...
    }
}

// Trait implementations — signature is dictated by the trait
impl Handler for MyHandler {
    fn handle(&self, req: Request, ctx: Context, state: State, config: Config) {
        // ...
    }
}

// Macro-generated functions
macro_rules! make_fn {
    () => {
        fn generated(a: i32, b: i32, c: i32, d: i32, e: i32) {}
    };
}
```

## Configuration

```toml
[suggest_fn_builder]
threshold = 4
```

| Field | Type | Default | Description |
|---|---|---|---|
| `threshold` | `usize` | `4` | Minimum parameter count (excluding `self`) to trigger the lint |

## Implementation notes

### Lint pass

`LateLintPass::check_fn` — match `FnKind::Fn` items and methods. Count the function's parameter declarations, excluding `self` receivers. Check whether the function already has a `#[bon::builder]` attribute.

### Skip conditions

| Condition | Reason |
|---|---|
| `span.from_expansion()` | Macro-generated functions |
| Has `#[bon::builder]` attr | Already using builder |
| Trait impl methods | Signature is dictated by the trait |
| Extern functions | Signature is dictated by FFI |

### Diagnostic

```
warning: function `connect` has 5 parameters — consider adding `#[bon::builder]`
  --> src/net.rs:10:1
   |
10 | fn connect(host: &str, port: u16, timeout: u64, retries: u32, tls: bool) {
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: add `#[bon::builder]` to enable named parameters at call sites
```

### Config struct

```rust
#[derive(Deserialize)]
#[serde(default)]
pub struct SuggestFnBuilderConfig {
    pub threshold: usize,
}

impl Default for SuggestFnBuilderConfig {
    fn default() -> Self {
        Self { threshold: 4 }
    }
}
```

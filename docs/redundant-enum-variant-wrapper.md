# `redundant_enum_variant_wrapper`

**Level:** `deny`

Flags an enum's inherent associated functions when its entire construction API consists of functions that directly forward unchanged parameters to variants.

## Why

Tuple variants are already constructor functions, while struct and unit variants have direct construction syntax. When every constructor is a direct wrapper, those wrappers add API names without adding conversion, validation, defaults, or abstraction:

```rust
enum Message {
    Text(String),
    Error { code: u16, detail: String },
    Quit,
}

impl Message {
    fn text(value: String) -> Self {
        Self::Text(value)
    }

    fn error(code: u16, detail: String) -> Self {
        Self::Error { code, detail }
    }

    fn quit() -> Self {
        Self::Quit
    }
}
```

Call the variants directly:

```rust
let text = Message::Text(value);
let error = Message::Error { code, detail };
let quit = Message::Quit;
```

A tuple variant can also be passed anywhere a function with the same signature is expected:

```rust
let messages = values.map(Message::Text);
```

## Exact matching rules

The lint reports a function only when all of these conditions hold:

- It is an inherent associated function on an enum, not a method with a `self` receiver or a trait implementation.
- Its entire body is a tuple, struct, or unit variant constructor for that same enum. Extra expression-only braces and an explicit `return` are ignored.
- Every parameter is a simple binding.
- Every parameter is passed directly to the variant exactly once, with no other values or implicit coercions.
- The enum has no other nontrivial associated constructor returning `Self`, `Result<Self, _>`, or `Box<Self>`. Constructors are considered across all of the enum's inherent `impl` blocks.

Parameter and field names do not need to match. The check resolves HIR definitions and local bindings rather than comparing source text.

The rule covers the constructors that exist; it does not require every declared variant to have a helper. An enum with one direct wrapper and several variants without helpers is still eligible for the lint.

## Does not trigger

Useful construction behavior for one variant keeps every direct wrapper on the same enum out of scope. This allows a consistent named-constructor API:

```rust
impl Message {
    // Allowed because `not_found` makes this enum's construction API
    // nontrivial.
    fn text(value: String) -> Self {
        Self::Text(value)
    }

    // A semantic default.
    fn not_found(detail: String) -> Self {
        Self::Error { code: 404, detail }
    }

    // Validation or any other statement.
    fn checked_error(code: u16, detail: String) -> Self {
        assert!(code >= 400);
        Self::Error { code, detail }
    }
}
```

Conversion, validation, additional statements, implicit coercions, and fallible constructors such as `fn parse(...) -> Result<Self, E>` all make the enum nontrivial. An unrelated associated function whose return type is not `Self`, `Result<Self, _>`, or `Box<Self>` has no effect.

Trait implementations are exempt because the trait controls their API. Associated functions on another type that happen to return an enum are also exempt.

If a direct wrapper must remain for API compatibility, document that exception at the narrowest scope:

```rust
#[expect(
    redundant_enum_variant_wrapper,
    reason = "retained for compatibility with the version 1 API"
)]
fn text(value: String) -> Self {
    Self::Text(value)
}
```

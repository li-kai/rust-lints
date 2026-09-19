# `unsafe_send_thread_affine_drop`

**Level:** `warn`

Flags an `unsafe impl Send` when a value with an explicit thread-affine
destruction contract remains reachable by automatic drop glue.

## Why

`Send` permits a value to be moved to another thread and destroyed there. That
is incompatible with resources that must be released on a particular thread or
execution context, such as a main-thread framework object or an event-loop
handle.

The old `unsafe_send_missing_drop` rule inferred this contract from `!Send` and
stopped warning when the outer type implemented `Drop`. Both implications were
incorrect:

- `!Send` can express aliasing or access constraints unrelated to destruction.
- `Drop::drop` runs before Rust automatically drops the type's fields. Merely
  implementing `Drop` does not remove a field from drop glue.

This replacement requires an explicit contract beside a local type and checks
the property that contract actually describes.

## Declaring a local contract

Add the companion macro crate to the workspace once:

```toml
[workspace.dependencies]
rust-lints-contracts = { git = "https://github.com/li-kai/rust-lints" }
```

Each member crate that declares a contract opts into that workspace dependency:

```toml
[dependencies]
rust-lints-contracts.workspace = true
```

Then place the contract on the type it describes:

```rust
#[rust_lints_contracts::thread_affine_drop(
    reason = "must be released on its creating thread"
)]
pub struct MainThreadHandle {
    // Keep the type structurally `!Send` so crossing the boundary always
    // requires an explicit `unsafe impl Send`.
    _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
}
```

The required reason is included in diagnostics. The attribute records metadata
for the lint; it does not implement `Send`, alter drop behavior, or perform
cleanup. Keeping the declaration next to the type avoids duplicated
`crate::...` paths in a monorepo and makes the contract travel with moves and
renames.

## Dependency-owned types

There are deliberately no built-in types. Rust has no trait meaning “must be
destroyed on its creating thread,” and a consumer cannot annotate a type owned
by another crate. List only those dependency types in `dylint.toml`:

```toml
[[unsafe_send_thread_affine_drop.external_types]]
path = "objc2::rc::Retained"
reason = "this application releases retained UI objects on the main dispatch queue"
```

Use the fully qualified definition path. Prefer an annotated local newtype when
the destruction contract applies only to some uses of a broad dependency type.

The source attribute and each external entry are independent contracts. No
contract enables, disables, or changes another lint rule.

## What it checks

For each explicit `unsafe impl Send for T`, the lint:

1. Checks whether `T` itself has a local contract or matches an external one.
2. Walks every field that Rust will destroy automatically.
3. Follows ordinary ADT fields, tuples, arrays, `Box`, `Vec`, `Rc`, `Arc`, and
   standard collections.
4. Reports the field, ownership path, declared type, and contract reason.

Traversal stops at:

- `ManuallyDrop<T>`, which structurally removes `T` from automatic drop glue;
- `PhantomData<T>`, which owns no value;
- references and raw pointers, which do not destroy their referent;
- unions, whose fields are not automatically dropped.

An arbitrary custom container that owns values only through raw pointers cannot
be inferred from its fields. Annotate that local container, or configure it as
an external type, if its own destruction contract is thread-affine.

Traversal uses a breadth-first search limited to 64 ownership edges in depth and
4,096 examined edges per top-level field. These limits bound expanding recursive
generic types and wide ownership graphs. Contracts on already queued types are
still checked after the work limit is reached. Hazards beyond either limit may
be missed; silence does not prove that an implementation is safe.

## Examples

### Fires even with an outer `Drop` implementation

```rust
struct Handle {
    resource: MainThreadHandle,
}

unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        // This does not prevent `resource` from being dropped immediately
        // after this method returns.
    }
}
```

### Fires through owning wrappers

```rust
struct Handles {
    resources: Vec<Option<Box<MainThreadHandle>>>,
}

unsafe impl Send for Handles {}
```

### Does not fire for explicit destruction storage

```rust
struct Handle {
    resource: ManuallyDrop<MainThreadHandle>,
}

unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        // Transfer or destroy `resource` in the context required by the
        // declared contract.
    }
}
```

The lint proves only that automatic drop glue cannot destroy the
`ManuallyDrop` value. Correct explicit cleanup remains part of the unsafe
implementation's safety argument.

### Does not infer affinity from `!Send`

```rust
struct Wrapper<T> {
    value: T,
}

unsafe impl<T> Send for Wrapper<T> {}
```

This may deserve a broader unsafe-auto-trait audit, but it is not evidence of a
thread-affine destruction contract. Clippy's `non_send_fields_in_send_ty`
covers that broader policy.

## Related lints

- `panic_in_drop` checks panic sites during destruction.
- Clippy's `non_send_fields_in_send_ty` audits the broader structural mismatch
  between `unsafe impl Send` and non-`Send` fields.

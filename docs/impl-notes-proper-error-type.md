# Implementation Notes for `proper_error_type` lint

Notes for implementing the lint described in `docs/proper-error-type.md`.

## Project layout

- Lint code: `src/lints/proper_error_type.rs`, add `pub mod proper_error_type;` in `src/lints/mod.rs`
- Registration: `src/lib.rs` — add to `register_lints` array + `register_late_pass`
- UI test: `ui/proper_error_type/main.rs` + `main.stderr`
- Cargo.toml: add `[[example]] name = "proper_error_type" path = "ui/proper_error_type/main.rs"`
- No config needed (no threshold or tunables)

### Registration (in `src/lib.rs`)

```rust
// Add to extern crates at top of file (already present: rustc_hir, rustc_lint, etc.)
// No new extern crates needed beyond what's already declared.

// In register_lints():
lint_store.register_lints(&[
    // ...existing...
    lints::proper_error_type::PROPER_ERROR_TYPE,
]);
lint_store.register_late_pass(|_| Box::new(lints::proper_error_type::ProperErrorType::default()));
```

## clippy_utils helpers (use these, don't reimplement)

```rust
// Return type of a function — erases late-bound regions automatically
clippy_utils::return_ty(cx, owner_id: OwnerId) -> Ty<'tcx>

// True when def_id is a method inside a trait impl (skip: signature is dictated by trait)
clippy_utils::is_def_id_trait_method(cx, def_id: LocalDefId) -> bool

// True when the function is main()
clippy_utils::is_entrypoint_fn(cx, def_id: DefId) -> bool

// True when ty implements the given trait (use for "does this field impl Error?")
clippy_utils::ty::implements_trait(cx, ty: Ty<'tcx>, trait_def_id: DefId, args: &[GenericArg]) -> bool
```

Note: `return_ty` takes `OwnerId`, construct via `OwnerId { def_id }` from `check_fn`'s `LocalDefId` parameter.

## Diagnostic items and symbols

All lookups use `cx.tcx.get_diagnostic_item(sym::X)` or `cx.tcx.is_diagnostic_item(sym::X, def_id)`.

| What | Symbol | API |
|---|---|---|
| `std::error::Error` trait | `sym::Error` | `get_diagnostic_item` (for trait DefId) |
| `std::fmt::Display` trait | `sym::Display` | `get_diagnostic_item` (for trait DefId) |
| `core::result::Result` | `sym::Result` | `is_diagnostic_item` on ADT's `did()` |
| `String` | `sym::String` | `is_diagnostic_item` on ADT's `did()` |
| `Box` | `sym::owned_box` | `is_diagnostic_item` on ADT's `did()` |
| `Cow` | `sym::Cow` | `is_diagnostic_item` on ADT's `did()` |

All of the above exist in `rustc_span::sym`.

**Not pre-defined — use `Symbol::intern`:**
- `Symbol::intern("source")` — for the `source` method name
- `Symbol::intern("anyhow")` — crate name check
- `Symbol::intern("miette")` — crate name check
- `Symbol::intern("Report")` — miette's error type name

## Key rustc APIs

```rust
// Visibility checks
cx.tcx.visibility(def_id.to_def_id()).is_public()         // syntactically pub
cx.tcx.effective_visibilities(()).is_reachable(local_def_id) // effectively public (for anyhow/miette)

// External crate type detection
cx.tcx.crate_name(def_id.krate) == Symbol::intern("anyhow")
cx.tcx.item_name(def_id)        == Symbol::intern("Error")   // anyhow::Error
cx.tcx.crate_name(def_id.krate) == Symbol::intern("miette")
cx.tcx.item_name(def_id)        == Symbol::intern("Report")  // miette::Report

// Type of a definition (for getting a type from an item's DefId)
cx.tcx.type_of(def_id).instantiate_identity() -> Ty<'tcx>

// ADT field iteration
adt_def.variants().iter() -> impl Iterator<Item = &VariantDef>
variant.fields.iter()     -> impl Iterator<Item = &FieldDef>
field.ty(cx.tcx, args)   -> Ty<'tcx>   // args from the ADT's generic args

// HIR access
cx.tcx.hir_impl_item(impl_item_id) -> &ImplItem
cx.tcx.hir_body(body_id)           -> &Body
cx.tcx.typeck(local_def_id)        -> &TypeckResults
```

## HIR shapes (nightly-2026-01-22)

```rust
ItemKind::Fn { sig, ident, generics, body, has_body }
ItemKind::Struct(ident, generics, variant_data)
ItemKind::Enum(ident, generics, enum_def)
ItemKind::Impl(impl_)  // impl_: &Impl

Impl { of_trait: Option<&TraitImplHeader>, self_ty, items: &[ImplItemId], .. }
TraitImplHeader { trait_ref: TraitRef, .. }
TraitRef { path }.trait_def_id() -> Option<DefId>

ImplItem { ident, owner_id, kind: ImplItemKind, .. }
ImplItemKind::Fn(FnSig, BodyId)
```

## LateLintPass callbacks used

```rust
fn check_fn(cx, kind: FnKind, decl: &FnDecl, body: &Body, span: Span, def_id: LocalDefId)
fn check_item(cx, item: &Item)
fn check_crate_post(cx)  // no extra args
```

## Architecture

### Struct state

```rust
#[derive(Default)]
pub struct ProperErrorType {
    /// `impl Error for T` blocks seen, keyed by the self type's ADT DefId.
    error_impls: FxHashMap<DefId, ErrorImplInfo>,
    /// `impl Display for T` blocks seen, keyed by the self type's ADT DefId.
    display_impls: FxHashMap<DefId, DisplayImplInfo>,
}

struct ErrorImplInfo {
    span: Span,
    has_source: bool,
    /// Fields of self type that implement Error (for step 3 cross-ref).
    source_field_names: Vec<Symbol>,
}

struct DisplayImplInfo {
    span: Span,
    /// LocalDefId of the `fmt` method, for body inspection in step 3.
    fmt_def_id: Option<LocalDefId>,
}
```

Use `FxHashMap` from `rustc_data_structures::fx` (re-exported by clippy_utils).

### Step 1 — Unstructured error types (`check_fn`)

Fires for every function (free functions, methods, associated functions).

**Skip conditions (return early):**
1. `span.from_expansion()` — macro-generated code
2. `is_def_id_trait_method(cx, def_id)` — trait impls have signatures dictated by the trait
3. `is_entrypoint_fn(cx, def_id.to_def_id())` — `fn main()` commonly uses `anyhow::Result`
4. `!cx.tcx.visibility(def_id.to_def_id()).is_public()` — private functions may use `String` errors
5. Inside `#[cfg(test)]` — check via `cx.tcx.hir_attrs(cx.tcx.local_def_id_to_hir_id(def_id))` for a `cfg` attr containing `test`, or walk parent modules

**Core logic:**
```rust
let ret_ty = clippy_utils::return_ty(cx, OwnerId { def_id });
if let ty::Adt(adt, args) = ret_ty.kind()
    && cx.tcx.is_diagnostic_item(sym::Result, adt.did())
{
    let err_ty = args.type_at(1);
    // Check err_ty against the unstructured patterns below
}
```

**Type matching on `err_ty`:**

| Pattern | Check |
|---|---|
| `String` | `ty::Adt(adt, _)` where `is_diagnostic_item(sym::String, adt.did())` |
| `&str` | `ty::Ref(_, inner, _)` where `inner.is_str()` |
| `Cow<'_, str>` | `ty::Adt(adt, args)` where `is_diagnostic_item(sym::Cow, adt.did())` and `args.type_at(0).is_str()` |
| `Box<dyn Error>` | `ty::Adt(adt, args)` where `is_diagnostic_item(sym::owned_box, adt.did())`, then `args.type_at(0)` is `ty::Dynamic(preds, ..)` and any `ExistentialPredicate::Trait(t)` has `t.def_id` matching the Error trait DefId |
| `anyhow::Error` | `ty::Adt(adt, _)` where `crate_name == "anyhow"` and `item_name == "Error"` |
| `miette::Report` | `ty::Adt(adt, _)` where `crate_name == "miette"` and `item_name == "Report"` |

**anyhow/miette extra gate:** Only flag if the function is effectively public:
```rust
cx.tcx.effective_visibilities(()).is_reachable(def_id)
```
A `pub fn` inside a `pub(crate)` module is syntactically public but not effectively public — skip it for anyhow/miette (it's acceptable in internal code).

For String/&str/Cow/Box<dyn Error>, syntactic `pub` visibility is sufficient to trigger.

### Steps 2-4 — `impl Error` / `impl Display` (`check_item` + `check_crate_post`)

All three steps process `ItemKind::Impl` blocks in `check_item` to collect data, then steps 3 and 4 emit in `check_crate_post` by cross-referencing.

#### In `check_item`:

**Skip conditions:**
1. `item.span.from_expansion()` — macro-generated code (catches thiserror derives)
2. `impl_.of_trait` is `None` — inherent impl, not a trait impl

**Identify the trait:**
```rust
let ItemKind::Impl(impl_) = &item.kind else { return };
if item.span.from_expansion() { return; }
let Some(trait_header) = impl_.of_trait else { return };
let Some(trait_def_id) = trait_header.trait_ref.trait_def_id() else { return };
```

**Get the self type's ADT DefId:**
```rust
let self_ty = cx.tcx.type_of(item.owner_id.def_id).instantiate_identity();
let Some(adt_def) = self_ty.ty_adt_def() else { return };
let adt_did = adt_def.did();
```

**If trait is `Error`** (matches `get_diagnostic_item(sym::Error)`):

1. Check whether `source()` is overridden: scan `impl_.items` for a method with `ident.name == Symbol::intern("source")`.
2. If `source()` is absent, check whether the type has fields implementing Error. Iterate `adt_def.variants()`, then `variant.fields`, get `field.ty(cx.tcx, args)`, and test `implements_trait(cx, field_ty, error_trait_id, &[])`. If any field implements Error, **emit step 2 immediately**.
3. Collect into `self.error_impls` with `has_source` and `source_field_names`.

**If trait is `Display`** (matches `get_diagnostic_item(sym::Display)`):

1. Find the `fmt` method in `impl_.items`, record its `LocalDefId`.
2. Collect into `self.display_impls`.

#### Thiserror detection

The `from_expansion()` check on the impl item's span is the primary mechanism. `thiserror` generates `impl Display` and `impl Error` via procedural macro — those impl blocks have `span.from_expansion() == true`. This means thiserror-derived types will never be collected into the HashMaps, and are thus exempt from steps 2-4 automatically.

Step 5 also needs thiserror detection. Since the struct/enum definition itself is NOT macro-generated (only the impls are), step 5 checks whether `implements_trait` returns true — if thiserror generated an `impl Error`, trait resolution will find it.

#### In `check_crate_post`:

**Step 4 — Manual Error + Display:**
Iterate `self.error_impls`. For each `(adt_did, error_info)`, check if `self.display_impls` also contains `adt_did`. If both are present, emit:
```
warning: manual `Error` + `Display` impl — use `#[derive(thiserror::Error)]`
```
Span on the Error impl (`error_info.span`).

**Step 3 — Duplicated source in Display:**
For each type present in BOTH maps where `error_info.has_source == true`:
1. Get the `fmt` method's `LocalDefId` from `display_info.fmt_def_id`.
2. Get the body: `cx.tcx.hir_body(cx.tcx.hir_body_owned_by(fmt_def_id))`.
3. Get typeck results: `cx.tcx.typeck(fmt_def_id)`.
4. Walk the body with a HIR visitor looking for field accesses to error-typed fields.

**Step 3 visitor sketch:**
```rust
struct SourceInDisplayVisitor<'a, 'tcx> {
    cx: &'a LateContext<'tcx>,
    typeck: &'tcx TypeckResults<'tcx>,
    source_field_names: &'a [Symbol],
    found_span: Option<Span>,
}

impl<'tcx> Visitor<'tcx> for SourceInDisplayVisitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        // Look for ExprKind::Field where the field name is in source_field_names
        // AND the field's type implements Error.
        // Also look for ExprKind::MethodCall of Display/Debug formatting methods
        // where the receiver is an error-typed field.
        if let ExprKind::Field(_, ident) = expr.kind {
            if self.source_field_names.contains(&ident.name) {
                self.found_span = Some(expr.span);
            }
        }
        rustc_hir::intravisit::walk_expr(self, expr);
    }
}
```

If the visitor finds a match, emit:
```
warning: `Display` renders inner error that is also returned by `source()`
```

### Step 5 — `*Error` without Error impl (`check_item`)

Fires for `ItemKind::Struct` and `ItemKind::Enum`.

**Skip conditions:**
1. `item.span.from_expansion()`
2. Name does not end with `"Error"` or `"Err"` — use `ident.as_str().ends_with("Error") || ident.as_str().ends_with("Err")`
3. `!cx.tcx.visibility(item.owner_id.to_def_id()).is_public()` — private types are fine
4. `#[cfg(test)]` context

**Core logic:**
```rust
let ty = cx.tcx.type_of(item.owner_id.def_id).instantiate_identity();
let Some(error_trait_id) = cx.tcx.get_diagnostic_item(sym::Error) else { return };
if !implements_trait(cx, ty, error_trait_id, &[]) {
    // emit warning
}
```

If `implements_trait` returns true (including via thiserror-generated impls), skip.

## Diagnostics

Use `clippy_utils::diagnostics::span_lint_and_help` for all steps, matching the existing lint patterns in this codebase.

**Step 1 — unstructured:**
```rust
span_lint_and_help(
    cx, PROPER_ERROR_TYPE, err_ty_span,
    format!("public function returns `Result<_, {err_ty_name}>` — use a type that implements `Error`"),
    None,
    "define an error enum with `#[derive(thiserror::Error)]`",
);
```

For the span: use `decl.output.span()` from `check_fn`'s `FnDecl` parameter to point at the return type.

**Step 1 — anyhow/miette:**
```rust
span_lint_and_help(
    cx, PROPER_ERROR_TYPE, err_ty_span,
    format!("effectively public function returns `{crate_name}::Error` — use a typed error"),
    None,
    "define an error enum with `#[derive(thiserror::Error)]` for library API surfaces",
);
```

**Step 2 — missing source:**
```rust
span_lint_and_help(
    cx, PROPER_ERROR_TYPE, error_impl_span,
    format!("`{type_name}` wraps error types but does not implement `source()`"),
    None,
    "use `#[derive(thiserror::Error)]` with `#[source]` / `#[from]`",
);
```

**Step 3 — duplicated source:**
```rust
span_lint_and_help(
    cx, PROPER_ERROR_TYPE, found_span,
    "`Display` renders inner error that is also returned by `source()`",
    None,
    "describe only what went wrong at this level — the source chain handles the rest",
);
```

**Step 4 — manual Error + Display:**
```rust
span_lint_and_help(
    cx, PROPER_ERROR_TYPE, error_impl_span,
    "manual `Error` + `Display` impl — use `#[derive(thiserror::Error)]`",
    None,
    "thiserror eliminates boilerplate and keeps Display in sync with variants",
);
```

**Step 5 — *Error without Error impl:**
```rust
span_lint_and_help(
    cx, PROPER_ERROR_TYPE, item.span,
    format!("`{type_name}` is named as an error type but does not implement `std::error::Error`"),
    None,
    "add `#[derive(thiserror::Error, Debug)]`",
);
```

## Lint declaration

```rust
rustc_session::declare_lint! {
    /// Flags error types in public APIs that are incomplete, unstructured,
    /// or missing error chain information.
    pub PROPER_ERROR_TYPE,
    Warn,
    "flags improper error types in public API signatures"
}

rustc_session::impl_lint_pass!(ProperErrorType => [PROPER_ERROR_TYPE]);
```

All five steps share a single lint (`PROPER_ERROR_TYPE`). This keeps configuration simple — users `#[allow(proper_error_type)]` to suppress any sub-check.

## Suggested implementation order

1. **Step 1** (unstructured errors) — most self-contained, only needs `check_fn`
2. **Step 5** (*Error without impl) — simple `check_item`, no cross-referencing
3. **Step 2** (missing source) — needs `check_item` for `Impl`, emits immediately
4. **Step 4** (manual Error + Display) — needs `check_crate_post` cross-ref, but no body walking
5. **Step 3** (duplicated source) — most complex, needs body visitor

Write UI tests incrementally: add test cases for each step as you implement it.

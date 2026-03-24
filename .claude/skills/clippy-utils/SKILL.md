---
name: clippy-utils
description: Reference for clippy_utils and rustc internals used in this repo's lint implementations. Use when writing new lints, modifying existing lints, or choosing between rustc/clippy_utils APIs for type checking, path matching, or HIR traversal.
user-invocable: false
---

# clippy_utils 0.1.95 (nightly 1.95) — Available APIs

This repo pins `clippy_utils 0.1.95` on `rustc 1.95.0-nightly`. Many APIs from older clippy_utils versions have been removed or inlined. Always verify availability before suggesting an import.

## Confirmed available imports

These are used successfully across lints in this repo:

| Import | Purpose |
|---|---|
| `clippy_utils::diagnostics::span_lint_and_help` | Emit lint with a help message |
| `clippy_utils::diagnostics::span_lint_and_then` | Emit lint with structured sub-diagnostics |
| `clippy_utils::is_entrypoint_fn` | Check if a function is `main` |
| `clippy_utils::is_trait_impl_item` | Check if an item is inside a trait impl (any trait) |
| `clippy_utils::is_def_id_trait_method` | Check if a `DefId` is a trait method |
| `clippy_utils::is_in_test` | Check if code is inside a `#[test]` function |
| `clippy_utils::is_in_cfg_test` | Check if code is inside `#[cfg(test)]` |
| `clippy_utils::return_ty` | Get the return type of a function |
| `clippy_utils::path_to_local_with_projections` | Resolve a path to a local variable with field projections |
| `clippy_utils::ty::implements_trait` | Check if a type implements a given trait |
| `clippy_utils::visitors::for_each_expr_without_closures` | Walk expressions, automatically skipping closures/async blocks |

## Removed / unavailable APIs

These do NOT exist in clippy_utils 0.1.95. Do not suggest them:

| Removed API | What to use instead |
|---|---|
| `clippy_utils::match_def_path` | Use `cx.tcx.is_diagnostic_item(sym::Name, def_id)`, `cx.tcx.is_lang_item(def_id, LangItem::Name)`, or `cx.tcx.def_path_str(def_id) == "std::path::to::item"` as a last resort |
| `clippy_utils::ty::is_type_diagnostic_item` | Inline it: `ty.peel_refs()` then match `ty::Adt(adt, _)` and call `cx.tcx.is_diagnostic_item(sym::Name, adt.did())` |
| `clippy_utils::ty::is_type_lang_item` | Same pattern as above but with `cx.tcx.is_lang_item(adt.did(), LangItem::Name)` |
| `TyCtxt::trait_of_item` | Walk the parent HIR node manually: `parent_hir_id` -> `Node::Item` -> `ItemKind::Impl` -> `of_trait` |

## Common patterns

### Check if an impl is for a specific trait

There is no shortcut. Walk the HIR parent manually:

```rust
let parent_id = cx.tcx.parent_hir_id(impl_item.hir_id());
let Node::Item(item) = cx.tcx.hir_node(parent_id) else { return false };
let rustc_hir::ItemKind::Impl(impl_block) = &item.kind else { return false };
let Some(trait_header) = impl_block.of_trait else { return false };
let Some(trait_def_id) = trait_header.trait_ref.trait_def_id() else { return false };
cx.tcx.is_lang_item(trait_def_id, LangItem::Drop)
```

### Check if a type is Option or Result

```rust
let ty = typeck.expr_ty_adjusted(receiver).peel_refs();
if let rustc_middle::ty::Adt(adt, _) = ty.kind() {
    let did = adt.did();
    cx.tcx.is_diagnostic_item(sym::Option, did)
        || cx.tcx.is_diagnostic_item(sym::Result, did)
}
```

### Match a function by its def path (no diagnostic item available)

```rust
if let Some(def_id) = cx.qpath_res(qpath, expr.hir_id).opt_def_id() {
    cx.tcx.def_path_str(def_id) == "std::thread::panicking"
}
```

This is fragile but necessary when no `sym::` diagnostic item exists for the target.

### Walk a function body, skipping closures

Use `for_each_expr_without_closures` for simple cases. Use a manual `Visitor` impl (without `NestedFilter`) when you need stateful traversal like toggling suppression flags inside guarded branches.

### Detect macro expansions

```rust
if expr.span.from_expansion() {
    let expn_data = expr.span.ctxt().outer_expn_data();
    if let ExpnKind::Macro(_, macro_name) = &expn_data.kind {
        // macro_name.as_str() gives e.g. "panic", "assert_eq"
    }
}
```

Walk up the expansion chain via `expn_data.call_site` to find the outermost user-facing macro.

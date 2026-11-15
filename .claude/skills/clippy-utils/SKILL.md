---
name: clippy-utils
description: Reference for clippy_utils and rustc internals used in this repo's lint implementations. Use when writing new lints, modifying existing lints, or choosing between rustc/clippy_utils APIs for type checking, path matching, or HIR traversal.
user-invocable: false
---

# clippy_utils 0.1.100 (nightly 1.100) — lint-writing reference

This repo pins `clippy_utils 0.1.100` on `rustc 1.100.0-nightly`
(`nightly-2026-09-01`, see `rust-toolchain` and `Cargo.toml`). Many APIs from
older clippy_utils versions have been removed. **Verify availability before
suggesting an import** — check `src/lints/` for a working example first.

Examples below are cited by file and symbol name, not line number. Search for
the symbol.

## Confirmed imports

All paths below are rooted at `clippy_utils::`.

Predicates (`-> bool`):

| Import | Purpose |
|---|---|
| `is_entrypoint_fn` | Check if a function is `main` |
| `is_expr_default` | Check if an expression is `Default::default()` |
| `is_test_function` | Check if a fn has `#[test]` (incl. `#[tokio::test]`) |
| `is_in_test` | Check if code is inside a `#[test]` function |
| `is_in_cfg_test` | Check if code is inside `#[cfg(test)]` |
| `is_trait_impl_item` | Check if an item is inside any trait impl |
| `is_def_id_trait_method` | Check if a `DefId` is a trait method |
| `ty::implements_trait` | Check if a type implements a trait |

Other:

| Import | Purpose | Example |
|---|---|---|
| `diagnostics::span_lint_and_help` | Emit lint with a help message | `debug_remnants.rs` |
| `diagnostics::span_lint_and_then` | Emit lint with structured sub-diagnostics (multi-span, notes) | `panic_in_drop.rs`, `acyclic_modules.rs` |
| `fn_def_id` | Resolve a call expression to the callee's `DefId` | `map_init_then_insert.rs`, `blocking_in_async.rs` |
| `return_ty` | Get the return type of a function | `proper_error_type.rs` |
| `visitors::for_each_expr` | Walk expressions, descending into closures. Takes `cx.tcx` first: `for_each_expr(cx.tcx, body, …)` | `realtime_in_async_test.rs` |
| `visitors::for_each_expr_without_closures` | Walk expressions, skipping closures | `proper_error_type.rs` |

### Shared helper modules (read these first)

- `src/lints/hir_refs.rs` — `resolve_expr_def_id`, `resolve_ty_def_id`,
  `def_path_segments`, `iife_closure_body`, `find_panic_macro`,
  `panicking_unwrap_or_expect`, `receiver_is_option_or_result`,
  `should_skip_ref`.
- `src/lints/call_matching.rs` — `match_call_path`, `find_matching_path`,
  `build_path_list`, `resolve_callee_def_id_with_typeck`,
  `is_in_suppression_zone`.
- `src/lints/suppression.rs` — `is_in_test_zone`.
- `src/lints/unnormalized.rs` — `type_of`, `fn_sig`, `field_ty`. Use these
  instead of calling `skip_norm_wip` directly.
- `src/lints/constructor.rs` — `return_adt`.

## Compiler API shapes on this toolchain

These changed between nightly 1.95 and 1.100. Code written from memory of the
older shapes will not compile.

| Area | Shape on 1.100 |
|---|---|
| Declaring a lint | `rustc_lint::declare_lint!` and `rustc_lint::impl_lint_pass!` (see `debug_remnants.rs`) |
| Registering a pass | Add the constructor to the `register_late_passes!` list in `src/lib.rs`. It expands to `lint_store.register_late_lint_pass(Box::new(\|_\| Box::new(pass)))` |
| `LangItem` | `rustc_hir::attrs::lang_items::LangItem` |
| `type_of` / `fn_sig` / `FieldDef::ty` | Return `ty::Unnormalized` (`type_of` and `fn_sig` after `instantiate_identity()`); go through `unnormalized::{type_of, fn_sig, field_ty}` |
| Visibility | `tcx.visibility` returns `ty::Visibility<ModId>`; test the crate root with `ModId::is_top_level_module` (see `vis_if_at_least_pub_crate` in `proper_error_type.rs`) |
| Struct-like HIR and def kinds | `ItemKind::Trait { .. }`, `DefKind::Const { .. }` (see `topological_ordering.rs`) |

## Removed / unavailable APIs

| Removed API | Replacement |
|---|---|
| `clippy_utils::match_def_path` | `cx.tcx.is_diagnostic_item(sym::Name, def_id)`, `cx.tcx.is_lang_item(def_id, LangItem::Name)`, or `cx.tcx.def_path_str(def_id) == "…"` as last resort |
| `clippy_utils::ty::is_type_diagnostic_item` | Inline: `ty.peel_refs()`, match `ty::Adt(adt, _)`, then `is_diagnostic_item(sym::Name, adt.did())` |
| `clippy_utils::ty::is_type_lang_item` | Same shape with `is_lang_item(adt.did(), LangItem::Name)` |
| `TyCtxt::trait_of_item` | Walk HIR parents — see `is_drop_impl` in `panic_in_drop.rs` |

## Task → file

| Task | Where to look |
|---|---|
| Check a type by diagnostic item (`Option`/`Result`) | `hir_refs::receiver_is_option_or_result` |
| Check a type by `LangItem` | `find_thread_affine_type` in `unsafe_send_thread_affine_drop.rs` (`ManuallyDrop`, `PhantomData`) |
| Match a third-party type with no diagnostic item | `recognized_map_type` in `map_init_then_insert.rs` |
| Check `ty: Trait` | `implements_trait` calls in `proper_error_type.rs` |
| Match a call against a configurable path list | `blocking_in_async.rs` (single set), `global_side_effect.rs` (multi-set) |
| Walk a body, simple | `check_duplicated_source` in `proper_error_type.rs` (`for_each_expr_without_closures`) |
| Walk a body, incl. closures + transitive callees | `has_transitive_time_call` in `realtime_in_async_test.rs` |
| Stateful body visitor | `DropPanicFinder` in `panic_in_drop.rs` (toggles on `panicking()` guard) |
| Visitor with nested body descent (`NestedFilter::OnlyBodies`) | `TimeCallVisitor` in `realtime_in_async_test.rs` |
| IIFE-only closure descent | `hir_refs::iife_closure_body`, used from `panic_in_drop.rs` / `fallible_new.rs` |
| Detect `impl Drop for T` (parent HIR walk) | `is_drop_impl` in `panic_in_drop.rs` |
| Detect `unsafe impl Send for T` (safety on `TraitImplHeader`) | `check_item` in `unsafe_send_thread_affine_drop.rs` |
| Detect a macro expansion, dedup by call site | `debug_remnants.rs`, `unstructured_log_fields.rs` |
| Walk expansion chain through internal panic helpers | `hir_refs::find_panic_macro` |
| Identify macro crate origin via `macro_def_id` | `find_tracing_macro_callsite` in `unstructured_log_fields.rs` |
| Walk HIR parents until item boundary | `is_in_async_context` and `is_inside_spawn_blocking` in `blocking_in_async.rs` |
| Resolve expr/ty/use to `DefId` for cross-module lints | `acyclic_modules.rs` (built entirely on `hir_refs::*`) |
| Inspect coroutine layout for await-held types | `check_interior_types` in `await_holding_unsendable.rs` |
| Detect `async fn` / `async {}` via desugared `Closure` | `is_in_async_context` in `blocking_in_async.rs`, `check_expr` in `await_holding_unsendable.rs` |
| One lint pass declaring multiple lints | `global_side_effect.rs` |
| Suppress in tests / `main` | `is_in_test_zone` / `is_in_suppression_zone` — ordered cheapest-first per `check_expr` in `blocking_in_async.rs` |
| Pre-expansion AST pass (name-only collection) | `bon_builder_collector.rs` + `lints/mod.rs` thread-locals |
| UI test harness | `testing.rs` + `crate::testing::run_ui_test`. Bless with `DYLINT_BLESS=1 cargo test <name>` |
| Register a new lint | `src/lib.rs`: add the lint to `register_lints` and the pass to the `register_late_passes!` list |

## Non-obvious gotchas

- `cx.typeck_results()` panics outside an active body. In `check_impl_item` /
  `check_item`, fetch via `cx.tcx.typeck(def_id)` and pass `&TypeckResults`
  down. `clippy_utils::fn_def_id` panics for the same reason in those
  contexts — use `call_matching::resolve_callee_def_id_with_typeck` or
  `cx.qpath_res(...).opt_def_id()` instead. See the doc comment on
  `is_panicking_guard` in `panic_in_drop.rs` and the helpers in `hir_refs.rs`
  that take an explicit `TypeckResults`.

- When recursing into another body's expressions, use *that* body's typeck
  results, not the caller's. See `has_transitive_time_call` in
  `realtime_in_async_test.rs`.

- `implements_trait` returns `false` for unbounded generics. A `false` on a
  generic parameter means "not provable for every `T`", not "does not
  implement".

- Path strings are normalized to strip turbofish `::<T>` segments before
  matching against configured paths — configure
  `tracing_subscriber::fmt::SubscriberBuilder::try_init`, not the generic
  form. See `strip_generic_args` in `call_matching.rs` (private helper).

- `impl_block.of_trait` carries `safety` and `trait_ref`, not `Impl` itself.
  See `check_item` in `unsafe_send_thread_affine_drop.rs`.

- `#[tokio::test(start_paused = true)]` attribute tokens are consumed before
  HIR exists. Detect by finding the generated `.start_paused(true)` call
  instead. See `realtime_in_async_test.rs` module docs.

## Performance conventions

Match these in new code.

1. Prefilter syntactically (`method.ident.as_str() == "…"`) before calling
   `def_path_str`, which allocates.
2. Intern `Symbol`s once in `new()`, cache on the lint-pass struct. Never
   `Symbol::intern` on a hot path. Example: `MapInitThenInsert::new` in
   `map_init_then_insert.rs`.
3. Run suppression-zone checks *after* the cheap match, not before.
   Example: `check_expr` in `global_side_effect.rs` and `blocking_in_async.rs`.
4. Dedup macro findings by call-site `Span` via `FxHashSet<Span>` on the
   pass struct. Example: `seen_callsites` in `debug_remnants.rs`.
5. Short-circuit visitors once all signals are collected.
   Example: `TimeCallVisitor::visit_expr` in `realtime_in_async_test.rs`.
6. In module-level lints, short-circuit intra-module references by `DefId`
   before allocating path segments. Example: `record_edge` in
   `acyclic_modules.rs`.

## Diagnostic item / LangItem cheat sheet

Reach for these before `def_path_str`.

**Diagnostic items** (`sym::…`, `is_diagnostic_item`, `get_diagnostic_item`):
`Option`, `Result`, `HashMap`, `BTreeMap`, `Cow`, `Error`, `Display`, `Send`.

**LangItems** (`LangItem::…`, `is_lang_item`): `Drop`, `String`, `OwnedBox`,
`ManuallyDrop`, `PhantomData`.

When neither exists: `crate_name` + `item_name` matching
(`recognized_map_type` in `map_init_then_insert.rs`), or `def_path_str`
string comparison as a last resort — fragile across compiler versions, so
cover with a UI test.

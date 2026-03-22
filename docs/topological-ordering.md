# Topological Ordering of Items Within a Module

## The Problem

When reading a Rust source file, the order of item definitions affects comprehension. If `main` calls `run`, which calls `process`, which calls `validate`, a reader benefits from a consistent ordering convention -- either always reading from high-level entry points down to implementation details, or from leaf utilities up to composition roots.

Without a convention, item order is accidental. Items end up wherever they were added chronologically, producing files where a reader must jump around to follow the call graph. This is worse in files maintained by multiple authors (human or AI), where no one is responsible for the whole file's narrative structure.

### Why a lint

Formatting tools (rustfmt) control whitespace and syntax style. They do not control item order because item order is semantic -- it depends on the call/reference graph between items. A lint with access to the HIR and type information can resolve which items reference which others and enforce a consistent ordering.

### Relationship to dylint's `non_topologically_sorted_functions`

dylint has an existing lint that covers functions only. This lint extends the concept to all items: structs, enums, traits, type aliases, constants, and their impl blocks. It also addresses grouping (a struct and its inherent impl should be treated as one unit) and provides a configurable direction choice.

## Design

### Direction: callee-first (bottom-up) as default

There are two valid conventions:

**Callee-first (bottom-up):** An item appears before any item that references it. Leaf functions are at the top of the file, composition roots at the bottom. This is the C convention (historically required by the compiler). Reading top-to-bottom reveals building blocks before their compositions.

**Caller-first (top-down):** An item appears before the items it references. Entry points are at the top of the file, implementation details at the bottom. This is the newspaper convention -- headline first, details later.

Trade-offs:

| Aspect | Callee-first | Caller-first |
|---|---|---|
| Familiar from | C, Go (convention), Pascal | Java (convention), newspaper style |
| Good for | Library modules with many leaf utilities | Application modules with clear entry points |
| First thing you see | Building blocks, types, helpers | Public API, orchestration |
| Matches Rust convention | Partially -- `main` is often last | Partially -- `mod` declarations are often first |
| dylint precedent | Yes -- dylint's lint uses callee-first | No |

**Decision:** Default to callee-first (matching dylint's precedent), configurable to caller-first. The rationale: callee-first has a natural affinity with Rust's type system -- types are defined before the functions that use them. It also matches the pattern where `main()` or `pub fn` entry points appear at the bottom of a file, after the internal machinery.

### What items are covered

All named items at the module level:

- Functions (`fn`)
- Structs (`struct`)
- Enums (`enum`)
- Traits (`trait`)
- Type aliases (`type`)
- Constants (`const`)
- Statics (`static`)
- Impl blocks (grouped with their type -- see below)

Not covered:

- `use` statements -- import ordering is rustfmt's job
- `mod` declarations -- module structure is architectural, not topological
- `extern crate` -- rare and always at the top
- Macro definitions (`macro_rules!`) -- they must appear before use regardless of convention
- Items inside function bodies -- too granular, not visible at module level

### Grouping: structs and their impl blocks

A struct/enum and its inherent `impl` blocks are logically one unit. Separating them with unrelated items is confusing. The lint treats each type and its inherent `impl` blocks as a single "item group" for ordering purposes.

**Specifically:**

1. A struct/enum and all its inherent `impl` blocks (not trait impls) are one group.
2. The group's position is determined by the struct/enum definition.
3. Inherent `impl` blocks must appear immediately after their struct/enum (no unrelated items in between).
4. Trait impl blocks (`impl Trait for Type`) are separate items. They reference both the trait and the type, and their ordering follows normal topological rules.

**Why only inherent impls:** Trait impls often exist to satisfy an external requirement (e.g., `impl Display for MyType`). Forcing them adjacent to the type definition would conflict with grouping trait impls together (e.g., all `Display` impls in one place). Inherent impls are the type's own API -- they belong with the type.

### What constitutes a "reference"

An edge from item A to item B exists when A's body or signature references B:

- **Function calls:** `validate(x)` in `process()` creates an edge `process -> validate`
- **Type annotations:** `fn process(x: Config)` creates an edge `process -> Config`
- **Struct literals:** `Config { ... }` creates an edge from the containing item to `Config`
- **Method calls:** `x.validate()` creates an edge to the impl item that defines `validate`
- **Trait bounds:** `fn process<T: Validate>(x: T)` creates an edge to `Validate`
- **Constants/statics in expressions:** `let x = MAX_SIZE;` creates an edge to `MAX_SIZE`

Only references to items in the **same module** are relevant. Cross-module references do not affect intra-module ordering.

### Cycle handling

Mutual references between items in the same module are common and legitimate:

```rust
fn parse_expr() -> Expr {
    // ...calls parse_atom...
}

fn parse_atom() -> Atom {
    // ...calls parse_expr for subexpressions...
}
```

When items form a cycle, the lint treats the entire strongly connected component (SCC) as a single unit. Items within an SCC can appear in any order relative to each other. The SCC as a whole must be ordered topologically relative to items outside the SCC.

This is the only correct approach. Reporting a cycle as an error would be wrong -- cycles are valid Rust and common in recursive-descent parsers, state machines, and mutually recursive data structures. Silently ignoring cycled items would leave ordering undefined. Treating the SCC as a unit preserves the ordering guarantee for everything outside the cycle.

### Algorithm

1. **Collect items.** During the lint pass, for each module, collect all top-level items with their spans and `DefId`s.

2. **Build the reference graph.** For each item, walk its HIR subtree (body, signature, where clauses) and resolve references to other items in the same module. Each resolved reference is a directed edge.

3. **Group inherent impls.** Merge each inherent `impl` block with its type definition into a single node.

4. **Compute SCCs.** Run Tarjan's algorithm (or Kosaraju's) to find strongly connected components. Collapse each SCC into a single node.

5. **Topological sort.** Sort the DAG of SCC nodes. This gives the expected order.

6. **Compare.** Walk the actual item order and the expected order. For each item that appears out of order, emit a diagnostic.

### Diagnostic format

```
warning: items are not in topological order in this module
  --> src/lib.rs:5:22
   |
5  |     fn process(_cfg: Config) {}
   |                      ^^^^^^ `fn process` references `struct Config` but appears before it
   |
help: reorder items topologically
   |
LL ~     struct Config {
LL +         value: u32,
LL +     }
LL +
LL + fn process(_cfg: Config) {}
   |
```

For the "item group" violation (inherent impl not adjacent to type):

```
warning: `impl Widget` is separated from its type definition
  --> src/lib.rs:30:5
   |
LL | /     struct Widget {
LL | |         name: String,
LL | |     }
   | |_____- `Widget` defined here
...
LL | /     impl Widget {
LL | |         fn new(name: String) -> Self {
LL | |             Self { name }
LL | |         }
LL | |     }
   | |_____^
   |
help: reorder items topologically
   |
   ...
```

### Autofix strategy

**Decision: provide `MachineApplicable` autofix via whole-module-body replacement.**

The lint emits a single suggestion per out-of-order module that replaces the entire module body with the correctly ordered items. `cargo fix` / `cargo dylint --fix` applies it automatically. This integrates with the pre-commit hook workflow (see [Pre-commit integration](#pre-commit-integration)).

#### Why whole-module replacement

Per-item suggestions (move item X to line Y) fail because:
- `rustfix` applies suggestions sequentially; moving one item invalidates spans of all subsequent items
- Multiple move suggestions within the same module produce overlapping spans, causing `rustfix` to abort

A single replacement of the entire module body sidesteps both problems -- one span, one replacement, no conflicts. This is conceptually the same approach rustfmt uses: rewrite the whole unit.

#### How it works

1. **Extract item text blocks.** For each item, use `cx.sess().source_map().span_to_snippet()` to get the source text. The span must include the item's leading attributes and doc comments. Use `cx.tcx.hir().attrs(item.hir_id())` to find the earliest attribute span and extend backward from there. For freestanding `//` comments immediately preceding an item (no blank line between), extend the span backward line-by-line through the source map.

2. **Compute the target order.** After SCC computation and topological sort, produce the permutation of items.

3. **Reassemble.** Concatenate the extracted text blocks in the new order, preserving the original blank-line spacing between items (one blank line between items, matching the original).

4. **Emit one suggestion.** Use `diag.span_suggestion()` with `Applicability::MachineApplicable` on the span covering the entire module body (from first item to last item), with the reassembled text as the replacement.

#### Edge cases and escape hatches

| Case | Behavior |
|---|---|
| **Macro-expanded items** | If any item in the module has a span from macro expansion (`span.from_expansion()`), skip autofix for that module. Still emit the warning with a note: "autofix skipped: module contains macro-expanded items". |
| **`#[cfg]` items** | Order what the current compilation sees. `#[cfg]` items absent from the current build are not in the HIR and cannot conflict. If a different cfg produces a different topological order, the lint fires again under that cfg and fixes it then. |
| **Comments between items** | Freestanding comments (not doc comments or attrs) that are separated from the next item by a blank line are treated as "section separators" and kept in place. Comments with no blank line before the next item travel with that item. |
| **`span_to_snippet` fails** | If the source map cannot produce a snippet for any item (e.g., synthetic spans), skip autofix for that module. |

#### Diagnostic format with autofix

```
warning: items are not in topological order in this module
  --> src/lib.rs:15:1
   |
15 | / fn process(x: Config) -> Output {
   | |     ...
42 | | fn validate(x: &Config) -> bool {
   | |     ...
   | |_
   |
   = note: `process` references `validate` (line 42) but appears before it
   = help: reorder items so referenced items appear first
   = note: autofix available: run `cargo fix` to reorder automatically
```

When `cargo fix` applies the suggestion, the entire module body is replaced with the correctly ordered version.

### Configuration

```toml
[topological_ordering]
# "callee_first" (default) or "caller_first"
direction = "callee_first"

# Whether to require inherent impl blocks adjacent to their type definition.
# Default: true
group_inherent_impls = true
```

Read from `dylint.toml` via `dylint_linting::config_or_default("topological_ordering")`.

No item exclusion configuration. Use `#[allow(topological_ordering)]` on specific items or modules where the convention does not apply (e.g., a module where items are ordered alphabetically by convention, or a module with extensive mutual recursion where the SCC is the entire module).

### Lint level

`Allow` by default — silent in the editor. Enable with `#![warn(topological_ordering)]` at crate root, or set `DYLINT_RUSTFLAGS="-W topological_ordering"` when running `cargo dylint`. This is a style/readability lint, not a correctness lint. Code with items in non-topological order compiles and runs correctly. Users who want stricter enforcement can set the level to `deny`.

## Resolved Questions

### 1. Why not just functions?

dylint's `non_topologically_sorted_functions` only covers functions. But types are part of the dependency graph. A function that takes a `Config` parameter depends on `Config`. If `Config` is defined 200 lines below the function, the reader cannot understand the function signature without scrolling. Covering all items makes the ordering holistic.

### 2. Why not enforce ordering of trait impls relative to the trait?

A trait impl (`impl Display for Config`) references both `Display` (external) and `Config` (local). Its position in the topological order is determined by these edges like any other item. Forcing it adjacent to `Config` would conflict with cases where a developer groups all `Display` impls together, or where `impl Trait for Type` depends on other local items. The topological ordering already places it correctly.

### 3. Why SCCs instead of just ignoring all cycles?

If items A and B form a cycle and items C and D are independent, ignoring cycles entirely would leave A and B unconstrained relative to C and D. Using SCCs preserves ordering guarantees: the {A, B} group is ordered relative to C and D, even though A and B are unordered relative to each other.

### 4. What about `pub` items?

Some codebases put all `pub` items first (or last). This lint does not enforce that convention. The topological order is determined by the reference graph, not visibility. A `pub` function that calls a private helper should appear after the helper (in callee-first mode), regardless of visibility. If a codebase wants visibility-based ordering, that is a different lint.

### 5. What about test modules?

`#[cfg(test)] mod tests { ... }` is excluded from ordering analysis. Test modules conventionally appear at the bottom of the file and reference all items in the module. Including them would force every item to appear before the test module (which is already the convention) but would not provide useful ordering signal within the test module itself. Items inside the test module could optionally be checked in a future extension.

### 6. Interaction with `acyclic_modules`

`acyclic_modules` checks dependencies between modules. `topological_ordering` checks item order within a module. They are orthogonal. Both can be active simultaneously without interference.

## File Structure

```
src/lints/
  topological_ordering.rs   # lint implementation
src/config.rs               # add TopologicalOrderingConfig

ui/topological_ordering/
  main.rs                   # UI test cases
  main.stderr               # expected diagnostics
```

Registration in `src/lib.rs`:
- Add `lints::topological_ordering::TOPOLOGICAL_ORDERING` to `register_lints`
- Add `register_late_pass` for `TopologicalOrdering`

Registration in `src/lints/mod.rs`:
- Add `pub mod topological_ordering;`

Add `[[example]]` entry in `Cargo.toml` for the UI test.

## Pre-commit integration

This lint is designed for the same pre-commit workflow as the auto-fixable clippy lints documented in `docs/recommended-lint-config.md`. Add to the pre-commit hook:

```bash
# Topological ordering — auto-reorders items within modules
DYLINT_RUSTFLAGS="-W topological_ordering" cargo dylint --all --fix -- --all-targets --allow-dirty --allow-staged
```

This runs after the clippy auto-fix step and before the check step. The workflow:

| Phase | Action |
|---|---|
| Development | Lint is `Allow` — silent in the editor |
| Pre-commit | `DYLINT_RUSTFLAGS="-W topological_ordering"` enables it for the fix pass |
| CI | `DYLINT_RUSTFLAGS="-W topological_ordering"` (or `-D`) catches violations |

The fix is idempotent: running it on already-ordered code produces no changes.

## Test Cases

1. **Basic callee-first ordering:** helper function before caller -- no warning.
2. **Basic violation:** caller before callee -- warning.
3. **Struct before function that uses it:** no warning.
4. **Function before struct it references:** warning.
5. **Inherent impl adjacent to struct:** no warning.
6. **Inherent impl separated from struct:** warning.
7. **Trait impl not required to be adjacent:** no warning regardless of position.
8. **Mutual recursion (cycle):** no warning for items in the SCC, but SCC ordered relative to other items.
9. **Multi-item SCC:** three mutually recursive functions, all unconstrained relative to each other.
10. **Constant referenced by function:** constant should appear before function.
11. **Type alias referenced in function signature:** alias should appear before function.
12. **`#[allow(topological_ordering)]` suppresses the warning.**
13. **`#[cfg(test)] mod tests` is excluded entirely.**
14. **Macro-expanded items are skipped.**
15. **Items with no references to other local items:** unconstrained, no warning regardless of position.
16. **Cross-module references do not create ordering edges.**
17. **Caller-first mode (configured):** reverses the expected order.

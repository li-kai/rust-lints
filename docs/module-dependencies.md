# Module Dependencies: Exploring Solutions to Architectural Erosion

## The Problem (Unsolved)

AI coding agents widen visibility modifiers (`fn` → `pub fn`, `pub(crate)` → `pub`) as a quick fix when encountering compiler access errors. This silently exposes internal implementation details and creates architectural debt. The question is: what's the best way to guide agents toward restructuring instead?

Several approaches exist, none proven optimal:

- **Enforce visibility narrowing directly:** Flag overly-wide `pub` and push back on widening. This works but requires the agent to understand why narrowing is right — it's fighting the symptom.
- **Enforce dependency direction:** Restrict which modules can depend on which. If the target module isn't permitted, widening visibility becomes useless and restructuring becomes necessary.
- **Enforce acyclic dependencies only:** Catch entanglement but not directionality. Weaker but simpler.
- **Split into crates:** Structural enforcement at compile time via `Cargo.toml`.
- **Do nothing in code:** Accept that this is an architectural decision, enforce it via code review and conventions.

This document explores one path: **enforcing dependency direction via an allowlist lint.** This is not the answer. It's a hypothesis, informed by research in other ecosystems, about what might work. The problem is unsolved. Multiple approaches remain viable, and the right answer may involve none of these tools.

## The Case for Module-Level Linting (Provisional)

Crate splitting solves the directional dependency problem structurally. `Cargo.toml` declares allowed dependencies, cargo enforces them, cycles are impossible. This is the structural solution.

But there's a question about timing and scope:

**Timing:** By the time a crate has grown enough to justify splitting, the architectural seams may already be tangled. A module-level lint earlier in the codebase's lifecycle could keep seams clean *before* splitting becomes necessary, making future refactoring cheaper.

**Scope:** Not every crate justifies a workspace. A 2000-line crate with 4 interdependent modules might never split. But it still benefits from enforced architectural structure.

The module-level lint is a hypothesis: apply crate-like discipline (directional dependencies) at the module scale. Whether this is better than "just split earlier" or "just review carefully" is unproven. This document explores what such a lint would look like.

## What We Learned From Other Ecosystems

We researched how Java (ArchUnit), Python (import-linter), TypeScript (dependency-cruiser), Go (language + third-party tools), and .NET (ArchUnitNET, NDepend) solve this problem. Key insights:

### What to Adopt

1. **Exhaustive mode** (from import-linter): Every module must be assigned to the config. An agent cannot create a new module that silently escapes enforcement.

2. **Full dependency chains in diagnostics** (from ArchUnit + import-linter): Show the exact line, the item being referenced, and which module it resolved to. The agent needs precision to know what to fix.

3. **Baseline file for adoption** (from ArchUnit's FreezingArchRule): Existing codebases have violations. Without a baseline, enabling the lint produces a wall of errors and gets disabled. A baseline records current violations and only flags new ones, making incremental adoption practical.

4. **Architecture-as-configuration** (from ArchUnit): The config file should be readable as the crate's architectural diagram. It's documentation that fails the build when violated.

5. **Full type resolution, not parse-only** (learning from cargo-archtest): Some tools only detect dependencies via `use` statements. They miss direct path references in function bodies, type annotations, and trait bounds. A compiler lint with `TyCtxt` access sees all resolved paths.

### What to Skip

1. **Transitive dependency checking** (from import-linter): import-linter flags `A → B → C` as `A` depending on `C`. We skip this because the allowlist already handles it — if both edges are approved, the transitive dependency is by design.

2. **Forbidden vs allowed duality** (from dependency-cruiser): Some tools support both denylists and allowlists. We use only allowlists — stricter and simpler.

3. **Multi-layer classification schemes**: Every ecosystem uses layers (Java: 3-5, Stripe: 5). Every engineer complains about classification ambiguity. The problem: logic and data have opposite gravity. A struct belongs in the foundation, but its methods may belong in application logic — they live in the same impl block. No layer scheme cleanly separates them. Per-module allowlists sidestep this by not classifying at all; they just constrain edges.

## One Possible Design: Allowlist-Based Module Dependencies

If the answer is "enforce module dependency direction," here's one design (informed by what works in other ecosystems):

**Lint name:** `module_dependencies`

**Core idea:** Each top-level module declares which other top-level modules it may depend on. Anything not in the allowlist is a violation.

**Configuration:**

```toml
[module_dependencies]
exhaustive = true  # Every module must appear in this config

[module_dependencies.allow]
types = []
errors = ["types"]
utils = ["types", "errors"]
payments = ["types", "errors", "utils"]
server = ["types", "errors", "utils", "payments"]
```

**How it would work:**

- At compile time, for every resolved path in the code, the lint checks: "Is the target module in the source module's allowlist?"
- If not, violation. Diagnostic shows the exact line, the item, and the resolved target.
- Modules not in the config are errors (exhaustive mode prevents escape hatches).
- `#[cfg(test)]` code is excluded — tests can reach anywhere.

**Baseline adoption (from ArchUnit):**

For existing codebases, generate a baseline file:

```toml
# module_dependencies_baseline.toml
[[accepted]]
from = "utils"
to = "payments"
reason = "Legacy; refactoring tracked in #1234"
```

New violations fail. Removing entries re-enables enforcement. This makes gradual adoption possible.

**Tradeoffs of this design:**

- **Pro:** Unambiguous (no classification debates like "is this a tool or a library?"). Exhaustive mode prevents modules from escaping enforcement. Full type resolution catches dependencies that parse-only tools miss.
- **Con:** Configuration burden (though with AI agents writing code, burden is one-time human setup). Doesn't solve the orphan rule problem when crates eventually split. May be overkill for small codebases that will never split.

## Open Questions

1. **Is directional enforcement the right leverage?** Our hypothesis is that restricting *which modules can depend on which* forces better restructuring than restrictions on *how visible items are*. But this is untested. It's possible that simpler approaches work better — just enabling `unreachable_pub`, or just doing code review, or just splitting into crates earlier.

2. **What's the false positive rate in practice?** The allowlist model is conceptually simple, but real codebases have patterns we haven't modeled — macros, generic code, cross-cutting concerns. How often would the lint fire on architecturally sound code?

3. **Does the configuration burden justify the value?** With AI agents, setup cost is one-time. But the config still needs to be right, and it needs to evolve as the codebase evolves. Is this worth it, or is the answer "just split into crates"?

4. **How does this interact with the orphan rule?** If you build this lint and later split the crate into a workspace, the orphan rule will create new constraints that the module-level lint never anticipated. Does this lint actually make crates-splitting easier, or just delay the pain?

5. **What do agents actually need?** When an agent encounters a visibility error or a dependency error, what guidance actually changes its behavior? A lint message? Access to the allowlist in the config? A guide explaining the orphan rule and how to handle it?

## What We Know From Other Ecosystems

- ArchUnit (Java) shows that architecture-as-tests works and scales.
- import-linter (Python) shows that exhaustive mode prevents drift.
- cargo-archtest (Rust) shows that parse-only dependency detection misses transitive dependencies.
- Existing Rust tooling offers `unreachable_pub` (visibility hygiene) and cycle detection, but nothing for directional enforcement.

This is the research foundation. The design above is one possible next step, informed but not proven.


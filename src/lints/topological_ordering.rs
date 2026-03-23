#![allow(
    clippy::indexing_slicing,
    reason = "graph algorithm indices are always in-bounds"
)]
#![allow(unclear_exports, reason = "internal lint crate, not a public API")]

//! Lint enforcing topological ordering of items within a module.
//!
//! Items that reference other items in the same module should appear in an
//! order consistent with their dependency graph.  By default, callees/referenced
//! items appear before callers/referencing items (callee-first / bottom-up).
//!
//! Cycles (mutual references) are handled by collapsing strongly connected
//! components into single nodes -- items within an SCC are unordered relative
//! to each other, but the SCC as a whole is ordered relative to outside items.
//!
//! **Autofix:** emits a single `MachineApplicable` suggestion per module that
//! replaces the entire module body with the correctly ordered items.  Applied
//! automatically by `cargo fix` / `cargo dylint --fix` in pre-commit.

use clippy_utils::diagnostics::span_lint_hir_and_then;
use clippy_utils::is_in_test;
use rustc_data_structures::fx::FxHashMap;
use rustc_errors::Applicability;
use rustc_hir as hir;
use rustc_hir::def::DefKind;
use rustc_lint::{LateContext, LateLintPass, LintContext as _};
use rustc_middle::ty::TyCtxt;
use rustc_span::Span;
use rustc_span::def_id::LocalDefId;

use super::hir_refs;
use crate::config::{OrderingDirection, TopologicalOrderingConfig};

rustc_session::declare_lint! {
    /// Flags items that appear out of topological order within a module.
    ///
    /// By default (callee-first), an item should appear before any item that
    /// references it.  Configurable to caller-first via `dylint.toml`.
    ///
    /// Cycles are tolerated: items in a strongly connected component are
    /// unordered relative to each other.
    ///
    /// Provides `MachineApplicable` autofix: `cargo fix` reorders items
    /// automatically.
    pub TOPOLOGICAL_ORDERING,
    Allow,
    "items are not in topological order within this module"
}

/// Represents a top-level item (or item group) within a single module,
/// tracked for ordering analysis.
struct ModuleItem {
    def_id: LocalDefId,
    span: Span,
    /// Human-readable name for diagnostics (e.g. "fn process", "struct Config").
    display_name: String,
    /// For inherent impl blocks, the `LocalDefId` of the self type.
    /// Used to group the impl with its type for ordering purposes.
    inherent_impl_self_ty: Option<LocalDefId>,
}

/// A raw reference collected during the lint pass, before resolution to
/// module-level items.
struct RawRef {
    /// The owner (function/method/const) containing the reference.
    source_owner: LocalDefId,
    /// The `DefId` being referenced (may be a nested item like a method).
    target: LocalDefId,
    /// Span of the reference site, for diagnostic labels.
    ref_span: Span,
}

/// Per-module collected data, built up during the lint pass and analyzed
/// in `check_crate_post`.
struct ModuleData {
    /// Span covering all items in the module body (first item start to last item end).
    /// Used as the replacement span for the autofix suggestion.
    body_span: Span,
    /// Items in source order.
    items: Vec<ModuleItem>,
    /// Whether any item has a span from macro expansion, which disables autofix.
    has_macro_items: bool,
}

pub struct TopologicalOrdering {
    config: TopologicalOrderingConfig,
    modules: FxHashMap<LocalDefId, ModuleData>,
    /// Raw reference edges collected during `check_expr` / `check_ty`.
    raw_refs: Vec<RawRef>,
    /// Cached lint-level check: `false` when lint is `Allow` at crate level.
    /// Set once in `check_crate`; when `false`, all callbacks short-circuit.
    enabled: bool,
}

impl TopologicalOrdering {
    pub fn new() -> Self {
        Self {
            config: dylint_linting::config_or_default("topological_ordering"),
            modules: FxHashMap::default(),
            raw_refs: Vec::new(),
            enabled: false,
        }
    }

    fn record_ref(
        &mut self,
        owner: LocalDefId,
        resolved: Option<(rustc_span::def_id::DefId, Span)>,
    ) {
        if let Some((def_id, span)) = resolved {
            if let Some(local_id) = def_id.as_local() {
                self.raw_refs.push(RawRef {
                    source_owner: owner,
                    target: local_id,
                    ref_span: span,
                });
            }
        }
    }
}

rustc_session::impl_lint_pass!(TopologicalOrdering => [TOPOLOGICAL_ORDERING]);

impl<'tcx> LateLintPass<'tcx> for TopologicalOrdering {
    fn check_crate(&mut self, cx: &LateContext<'tcx>) {
        self.enabled =
            !clippy_utils::is_lint_allowed(cx, TOPOLOGICAL_ORDERING, rustc_hir::CRATE_HIR_ID);
    }

    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx hir::Item<'tcx>) {
        if !self.enabled {
            return;
        }
        if !is_relevant_item_kind(&item.kind) {
            return;
        }

        let item_def_id = item.owner_id.def_id;

        // Only process direct children of modules.
        let parent_def_id = cx.tcx.parent(item_def_id.to_def_id());
        let Some(parent_local) = parent_def_id.as_local() else {
            return;
        };
        if cx.tcx.def_kind(parent_local) != DefKind::Mod {
            return;
        }

        // Skip items in test code.
        if is_in_test(cx.tcx, item.hir_id()) {
            return;
        }

        let from_expansion = item.span.from_expansion();
        let display_name = item_display_name(cx, item);

        // Determine if this is an inherent impl.
        let inherent_impl_self_ty = if let hir::ItemKind::Impl(impl_block) = &item.kind {
            if impl_block.of_trait.is_none() {
                resolve_self_ty_def_id(cx, impl_block)
            } else {
                None
            }
        } else {
            None
        };

        let module_data = self
            .modules
            .entry(parent_local)
            .or_insert_with(|| ModuleData {
                body_span: item.span,
                items: Vec::new(),
                has_macro_items: false,
            });

        if from_expansion {
            module_data.has_macro_items = true;
        }

        if module_data.items.is_empty() {
            module_data.body_span = item.span;
        } else {
            module_data.body_span = module_data.body_span.to(item.span);
        }

        module_data.items.push(ModuleItem {
            def_id: item_def_id,
            span: item.span,
            display_name,
            inherent_impl_self_ty,
        });
    }

    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx hir::Expr<'tcx>) {
        if !self.enabled {
            return;
        }
        if !expr.span.from_expansion() {
            let resolved = hir_refs::resolve_expr_def_id(cx, expr)
                .map(|(def_id, _hir_id, span)| (def_id, span));
            self.record_ref(expr.hir_id.owner.def_id, resolved);
        }
    }

    fn check_ty(&mut self, cx: &LateContext<'tcx>, ty: &'tcx hir::Ty<'tcx, hir::AmbigArg>) {
        if !self.enabled {
            return;
        }
        if !ty.span.from_expansion() {
            let resolved =
                hir_refs::resolve_ty_def_id(cx, ty).map(|(def_id, _hir_id, span)| (def_id, span));
            self.record_ref(ty.hir_id.owner.def_id, resolved);
        }
    }

    fn check_crate_post(&mut self, cx: &LateContext<'tcx>) {
        if !self.enabled {
            return;
        }

        let mut def_id_to_module_item: FxHashMap<LocalDefId, (LocalDefId, usize)> =
            FxHashMap::default();
        for (&module_def_id, module_data) in &self.modules {
            for (idx, item) in module_data.items.iter().enumerate() {
                def_id_to_module_item.insert(item.def_id, (module_def_id, idx));
            }
        }

        let mut module_refs: FxHashMap<LocalDefId, Vec<(usize, usize, Span)>> =
            FxHashMap::default();
        let mut resolve_cache: FxHashMap<LocalDefId, Option<(LocalDefId, usize)>> =
            FxHashMap::default();
        for raw_ref in &self.raw_refs {
            let source = *resolve_cache
                .entry(raw_ref.source_owner)
                .or_insert_with(|| {
                    find_module_item(cx.tcx, raw_ref.source_owner, &def_id_to_module_item)
                });
            let target = *resolve_cache.entry(raw_ref.target).or_insert_with(|| {
                find_module_item(cx.tcx, raw_ref.target, &def_id_to_module_item)
            });

            if let (Some((src_mod, src_idx)), Some((tgt_mod, tgt_idx))) = (source, target) {
                if src_mod == tgt_mod && src_idx != tgt_idx {
                    module_refs.entry(src_mod).or_default().push((
                        src_idx,
                        tgt_idx,
                        raw_ref.ref_span,
                    ));
                }
            }
        }

        for (&module_def_id, module_data) in &self.modules {
            if module_data.items.len() <= 1 {
                continue;
            }

            let resolved_refs = module_refs
                .get(&module_def_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);

            let item_def_id_to_idx = build_def_id_to_idx(&module_data.items);

            // Apply inherent-impl grouping to refs (merge impl refs with type).
            let remapped_refs = remap_inherent_impl_refs(
                &module_data.items,
                &item_def_id_to_idx,
                resolved_refs,
                self.config.group_inherent_impls,
            );

            let n = module_data.items.len();
            let adj = build_adj_list(&remapped_refs, n);
            let (item_to_scc, sccs) = compute_sccs(&adj, n);

            let ordering_violations = find_ordering_violations(
                &module_data.items,
                &remapped_refs,
                &item_to_scc,
                self.config.direction,
            );

            let grouping_violations = if self.config.group_inherent_impls {
                check_impl_grouping(&module_data.items, &item_def_id_to_idx)
            } else {
                Vec::new()
            };

            if ordering_violations.is_empty() && grouping_violations.is_empty() {
                continue;
            }

            let autofix = if module_data.has_macro_items {
                None
            } else {
                let expected_order = compute_expected_order(
                    &module_data.items,
                    &adj,
                    &item_to_scc,
                    &sccs,
                    self.config.direction,
                    self.config.group_inherent_impls,
                );
                compute_reordered_body(cx, module_data, &expected_order)
            };

            emit_module_diagnostic(
                cx,
                module_data,
                &ordering_violations,
                &grouping_violations,
                autofix.as_deref(),
            );
        }
    }
}

// Item classification helpers

fn is_relevant_item_kind(kind: &hir::ItemKind<'_>) -> bool {
    matches!(
        kind,
        hir::ItemKind::Fn { .. }
            | hir::ItemKind::Struct(..)
            | hir::ItemKind::Enum(..)
            | hir::ItemKind::Trait(..)
            | hir::ItemKind::TyAlias(..)
            | hir::ItemKind::Const(..)
            | hir::ItemKind::Static(..)
            | hir::ItemKind::Impl(..)
    )
}

fn item_display_name(cx: &LateContext<'_>, item: &hir::Item<'_>) -> String {
    let def_id = item.owner_id.to_def_id();
    let kind = cx.tcx.def_kind(def_id);

    if let DefKind::Impl { .. } = kind {
        if let hir::ItemKind::Impl(impl_block) = &item.kind {
            let ty_name = cx
                .sess()
                .source_map()
                .span_to_snippet(impl_block.self_ty.span)
                .unwrap_or_else(|_| "?".into());
            return if impl_block.of_trait.is_some() {
                format!("impl .. for {ty_name}")
            } else {
                format!("impl {ty_name}")
            };
        }
    }

    let name = cx
        .tcx
        .opt_item_name(def_id)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "?".into());

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only a few kinds get prefixes"
    )]
    let prefix = match kind {
        DefKind::Fn => "fn",
        DefKind::Struct => "struct",
        DefKind::Enum => "enum",
        DefKind::Trait => "trait",
        DefKind::TyAlias => "type",
        DefKind::Const => "const",
        DefKind::Static { .. } => "static",
        _ => "",
    };

    if prefix.is_empty() {
        name
    } else {
        format!("{prefix} {name}")
    }
}

fn resolve_self_ty_def_id(cx: &LateContext<'_>, impl_block: &hir::Impl<'_>) -> Option<LocalDefId> {
    if let hir::TyKind::Path(ref qpath) = impl_block.self_ty.kind
        && let hir::def::Res::Def(_, def_id) = cx.qpath_res(qpath, impl_block.self_ty.hir_id)
    {
        def_id.as_local()
    } else {
        None
    }
}

// Reference resolution

/// Walk up the parent chain from `def_id` until we find a module-level item.
fn find_module_item(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    map: &FxHashMap<LocalDefId, (LocalDefId, usize)>,
) -> Option<(LocalDefId, usize)> {
    if let Some(&result) = map.get(&def_id) {
        return Some(result);
    }
    let mut current = def_id.to_def_id();
    loop {
        let parent = tcx.parent(current);
        if parent == current {
            return None;
        }
        if let Some(local) = parent.as_local() {
            if let Some(&result) = map.get(&local) {
                return Some(result);
            }
        }
        current = parent;
    }
}

/// Build a map from `LocalDefId` → item index for O(1) lookups.
fn build_def_id_to_idx(items: &[ModuleItem]) -> FxHashMap<LocalDefId, usize> {
    items
        .iter()
        .enumerate()
        .map(|(idx, item)| (item.def_id, idx))
        .collect()
}

/// When `group_inherent_impls` is enabled, remap refs so that inherent impl
/// items are treated as part of their type definition.
fn remap_inherent_impl_refs(
    items: &[ModuleItem],
    def_id_to_idx: &FxHashMap<LocalDefId, usize>,
    refs: &[(usize, usize, Span)],
    group: bool,
) -> Vec<(usize, usize, Span)> {
    if !group {
        return refs.to_vec();
    }

    let remap = |idx: usize| -> usize {
        items[idx]
            .inherent_impl_self_ty
            .and_then(|self_ty| def_id_to_idx.get(&self_ty).copied())
            .unwrap_or(idx)
    };

    refs.iter()
        .map(|&(from, to, span)| (remap(from), remap(to), span))
        .filter(|(from, to, _)| from != to)
        .collect()
}

// Graph construction & SCC computation

fn build_adj_list(refs: &[(usize, usize, Span)], n: usize) -> Vec<Vec<usize>> {
    let mut adj = vec![Vec::new(); n];
    for &(from, to, _) in refs {
        adj[from].push(to);
    }
    for list in &mut adj {
        list.sort_unstable();
        list.dedup();
    }
    adj
}

fn compute_sccs(adj: &[Vec<usize>], n: usize) -> (Vec<usize>, Vec<Vec<usize>>) {
    let sccs = tarjan_scc(adj, n);
    let mut item_to_scc = vec![0usize; n];
    for (scc_idx, scc) in sccs.iter().enumerate() {
        for &item_idx in scc {
            item_to_scc[item_idx] = scc_idx;
        }
    }
    (item_to_scc, sccs)
}

// -- Tarjan's algorithm --

struct TarjanState {
    index_counter: usize,
    stack: Vec<usize>,
    on_stack: Vec<bool>,
    index: Vec<usize>,
    lowlink: Vec<usize>,
    sccs: Vec<Vec<usize>>,
}

fn tarjan_scc(adj: &[Vec<usize>], n: usize) -> Vec<Vec<usize>> {
    let mut state = TarjanState {
        index_counter: 0,
        stack: Vec::new(),
        on_stack: vec![false; n],
        index: vec![usize::MAX; n],
        lowlink: vec![0; n],
        sccs: Vec::new(),
    };
    for v in 0..n {
        if state.index[v] == usize::MAX {
            strongconnect(v, adj, &mut state);
        }
    }
    state.sccs
}

fn strongconnect(v: usize, adj: &[Vec<usize>], state: &mut TarjanState) {
    state.index[v] = state.index_counter;
    state.lowlink[v] = state.index_counter;
    state.index_counter += 1;
    state.stack.push(v);
    state.on_stack[v] = true;

    for &w in &adj[v] {
        if state.index[w] == usize::MAX {
            strongconnect(w, adj, state);
            state.lowlink[v] = state.lowlink[v].min(state.lowlink[w]);
        } else if state.on_stack[w] {
            state.lowlink[v] = state.lowlink[v].min(state.index[w]);
        }
    }

    if state.lowlink[v] == state.index[v] {
        let mut scc = Vec::new();
        while let Some(w) = state.stack.pop() {
            state.on_stack[w] = false;
            scc.push(w);
            if w == v {
                break;
            }
        }
        state.sccs.push(scc);
    }
}

// -- Topological sort with source-order tiebreaking --

fn topo_sort_stable(
    adj: &[Vec<usize>],
    source_pos: &[usize],
    n: usize,
    reverse_edges: bool,
) -> Vec<usize> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let mut effective_adj = vec![Vec::new(); n];
    let mut in_degree = vec![0u32; n];

    for (from, targets) in adj.iter().enumerate() {
        for &to in targets {
            if reverse_edges {
                effective_adj[to].push(from);
                in_degree[from] += 1;
            } else {
                effective_adj[from].push(to);
                in_degree[to] += 1;
            }
        }
    }

    let mut heap: BinaryHeap<Reverse<(usize, usize)>> = BinaryHeap::new();
    for i in 0..n {
        if in_degree[i] == 0 {
            heap.push(Reverse((source_pos[i], i)));
        }
    }

    let mut order = Vec::with_capacity(n);
    while let Some(Reverse((_, node))) = heap.pop() {
        order.push(node);
        for &next in &effective_adj[node] {
            in_degree[next] -= 1;
            if in_degree[next] == 0 {
                heap.push(Reverse((source_pos[next], next)));
            }
        }
    }

    order
}

// Violation detection

/// A single ordering violation: an item that appears at the wrong position.
struct OrderingViolation {
    /// `LocalDefId` of the out-of-order item (used for lint-level resolution).
    item_def_id: LocalDefId,
    /// Span of the first reference that demonstrates the violation.
    ref_span: Span,
    item_name: String,
    /// (referenced item name, ref_span)
    witnesses: Vec<(String, Span)>,
}

/// A grouping violation: an inherent impl separated from its type.
struct GroupingViolation {
    /// `LocalDefId` of the impl block (used for lint-level resolution).
    impl_def_id: LocalDefId,
    impl_span: Span,
    type_name: String,
    type_span: Span,
}

fn find_ordering_violations(
    items: &[ModuleItem],
    refs: &[(usize, usize, Span)],
    item_to_scc: &[usize],
    direction: OrderingDirection,
) -> Vec<OrderingViolation> {
    let mut violation_map: FxHashMap<usize, Vec<(String, Span)>> = FxHashMap::default();

    for &(from, to, ref_span) in refs {
        if item_to_scc[from] == item_to_scc[to] {
            continue;
        }

        let is_violation = match direction {
            OrderingDirection::CalleeFirst => from < to,
            OrderingDirection::CallerFirst => to < from,
        };

        if is_violation {
            violation_map
                .entry(from)
                .or_default()
                .push((items[to].display_name.clone(), ref_span));
        }
    }

    let mut violations: Vec<OrderingViolation> = violation_map
        .into_iter()
        .map(|(from_idx, witnesses)| OrderingViolation {
            item_def_id: items[from_idx].def_id,
            ref_span: witnesses[0].1,
            item_name: items[from_idx].display_name.clone(),
            witnesses,
        })
        .collect();
    violations.sort_by_key(|v| v.ref_span.lo());
    violations
}

fn check_impl_grouping(
    items: &[ModuleItem],
    def_id_to_idx: &FxHashMap<LocalDefId, usize>,
) -> Vec<GroupingViolation> {
    let mut violations = Vec::new();

    for (impl_pos, item) in items.iter().enumerate() {
        let Some(self_ty_def_id) = item.inherent_impl_self_ty else {
            continue;
        };
        let Some(&type_idx) = def_id_to_idx.get(&self_ty_def_id) else {
            continue;
        };
        let type_item = &items[type_idx];

        let (lo, hi) = if impl_pos > type_idx {
            (type_idx, impl_pos)
        } else {
            (impl_pos, type_idx)
        };
        let has_unrelated = items[lo + 1..hi]
            .iter()
            .any(|i| i.def_id != self_ty_def_id && i.inherent_impl_self_ty != Some(self_ty_def_id));

        if has_unrelated {
            violations.push(GroupingViolation {
                impl_def_id: item.def_id,
                impl_span: item.span,
                type_name: type_item
                    .display_name
                    .split_once(' ')
                    .map_or(type_item.display_name.as_str(), |(_, name)| name)
                    .to_string(),
                type_span: type_item.span,
            });
        }
    }

    violations
}

// Expected order & autofix

fn compute_expected_order(
    items: &[ModuleItem],
    adj: &[Vec<usize>],
    item_to_scc: &[usize],
    sccs: &[Vec<usize>],
    direction: OrderingDirection,
    group_inherent_impls: bool,
) -> Vec<usize> {
    let num_sccs = sccs.len();

    // Build SCC DAG.
    let mut scc_adj = vec![Vec::new(); num_sccs];
    for (from, targets) in adj.iter().enumerate() {
        for &to in targets {
            let from_scc = item_to_scc[from];
            let to_scc = item_to_scc[to];
            if from_scc != to_scc {
                scc_adj[from_scc].push(to_scc);
            }
        }
    }
    for list in &mut scc_adj {
        list.sort_unstable();
        list.dedup();
    }

    // Min item index per SCC for tiebreaking (index == source position).
    let scc_min_pos: Vec<usize> = sccs
        .iter()
        .map(|scc| scc.iter().copied().min().unwrap_or(0))
        .collect();

    let callee_first = matches!(direction, OrderingDirection::CalleeFirst);
    let scc_order = topo_sort_stable(&scc_adj, &scc_min_pos, num_sccs, callee_first);

    let mut item_order = Vec::with_capacity(items.len());
    for &scc_idx in &scc_order {
        let mut scc_items = sccs[scc_idx].clone();
        scc_items.sort_unstable();
        item_order.extend(scc_items);
    }

    if group_inherent_impls {
        // Build type_def_id → [impl indices] map for O(1) lookup.
        let mut type_to_impls: FxHashMap<LocalDefId, Vec<usize>> = FxHashMap::default();
        for &idx in &item_order {
            if let Some(self_ty) = items[idx].inherent_impl_self_ty {
                type_to_impls.entry(self_ty).or_default().push(idx);
            }
        }

        let mut final_order = Vec::with_capacity(items.len());
        let mut placed = vec![false; items.len()];

        for &idx in &item_order {
            if placed[idx] {
                continue;
            }
            // Skip inherent impls here; they will be placed after their type.
            if items[idx].inherent_impl_self_ty.is_some() {
                continue;
            }

            final_order.push(idx);
            placed[idx] = true;

            // Add inherent impls of this type.
            if let Some(impl_indices) = type_to_impls.get(&items[idx].def_id) {
                for &impl_idx in impl_indices {
                    if !placed[impl_idx] {
                        final_order.push(impl_idx);
                        placed[impl_idx] = true;
                    }
                }
            }
        }

        // Add any remaining items (orphaned impls whose type is in another module).
        for &idx in &item_order {
            if !placed[idx] {
                final_order.push(idx);
            }
        }

        final_order
    } else {
        item_order
    }
}

/// Compute the reordered module body as a string.
///
/// Returns `None` if any snippet extraction fails (e.g. synthetic spans).
fn compute_reordered_body(
    cx: &LateContext<'_>,
    module_data: &ModuleData,
    expected_order: &[usize],
) -> Option<String> {
    let source_map = cx.sess().source_map();

    // Check if the expected order is the same as the source order.
    let is_already_ordered = expected_order.iter().enumerate().all(|(i, &idx)| idx == i);
    if is_already_ordered {
        return None;
    }

    let mut snippets = Vec::with_capacity(module_data.items.len());
    for item in &module_data.items {
        let snippet = source_map.span_to_snippet(item.span).ok()?;
        snippets.push(snippet);
    }

    let reordered: Vec<&str> = expected_order
        .iter()
        .map(|&idx| snippets[idx].as_str())
        .collect();

    Some(reordered.join("\n\n"))
}

// Diagnostics

fn emit_autofix_or_help(
    diag: &mut rustc_errors::Diag<'_, ()>,
    module_data: &ModuleData,
    reordered_body: Option<&str>,
    manual_hint: &str,
) {
    if let Some(reordered) = reordered_body {
        diag.span_suggestion(
            module_data.body_span,
            "reorder items topologically",
            reordered,
            Applicability::MachineApplicable,
        );
    } else if module_data.has_macro_items {
        diag.help(format!(
            "autofix unavailable for this module (contains macro-expanded items); {manual_hint}",
        ));
    }
}

fn emit_module_diagnostic(
    cx: &LateContext<'_>,
    module_data: &ModuleData,
    ordering_violations: &[OrderingViolation],
    grouping_violations: &[GroupingViolation],
    reordered_body: Option<&str>,
) {
    if !ordering_violations.is_empty() {
        let first = &ordering_violations[0];
        let hir_id = cx.tcx.local_def_id_to_hir_id(first.item_def_id);

        span_lint_hir_and_then(
            cx,
            TOPOLOGICAL_ORDERING,
            hir_id,
            first.ref_span,
            "items are not in topological order in this module",
            |diag| {
                for violation in ordering_violations {
                    for (name, span) in &violation.witnesses {
                        diag.span_label(
                            *span,
                            format!(
                                "`{}` references `{name}` but appears before it",
                                violation.item_name,
                            ),
                        );
                    }
                }
                emit_autofix_or_help(
                    diag,
                    module_data,
                    reordered_body,
                    "reorder items manually so referenced items appear first",
                );
            },
        );
    }

    for violation in grouping_violations {
        let hir_id = cx.tcx.local_def_id_to_hir_id(violation.impl_def_id);

        span_lint_hir_and_then(
            cx,
            TOPOLOGICAL_ORDERING,
            hir_id,
            violation.impl_span,
            format!(
                "`impl {}` is separated from its type definition",
                violation.type_name
            ),
            |diag| {
                diag.span_label(
                    violation.type_span,
                    format!("`{}` defined here", violation.type_name),
                );
                if ordering_violations.is_empty() {
                    emit_autofix_or_help(
                        diag,
                        module_data,
                        reordered_body,
                        "reorder items manually so the impl is adjacent to its type",
                    );
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ui_topological_ordering() {
        crate::testing::run_ui_test("topological_ordering", None, &[]);
    }
}

use clippy_utils::diagnostics::span_lint_and_help;
use clippy_utils::path_to_local_with_projections;
use rustc_hir::{Block, Expr, ExprKind, HirId, PatKind, Stmt, StmtKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty::{self, Ty};
use rustc_span::{Symbol, sym};

// ── Lint declaration ────────────────────────────────────────────────

rustc_session::declare_lint! {
    /// Warns when a `HashMap`, `BTreeMap`, or `IndexMap` is created empty and
    /// then immediately populated with sequential `.insert()` calls.
    ///
    /// Suggests using `Type::from([...])` instead.
    pub MAP_INIT_THEN_INSERT,
    Warn,
    "immediately inserting into a newly created map \u{2014} consider using `Type::from([..])`"
}

pub struct MapInitThenInsert {
    // Cached symbols for IndexMap detection (no diagnostic item exists).
    // Interned once in `new()` to avoid per-statement re-interning.
    sym_indexmap_crate: Symbol,
    sym_indexmap_type: Symbol,
}

impl MapInitThenInsert {
    pub fn new() -> Self {
        Self {
            sym_indexmap_crate: Symbol::intern("indexmap"),
            sym_indexmap_type: Symbol::intern("IndexMap"),
        }
    }
}

rustc_session::impl_lint_pass!(MapInitThenInsert => [MAP_INIT_THEN_INSERT]);

/// Minimum number of consecutive `.insert()` calls required to fire the lint.
/// A single insert isn't worth rewriting.
const MIN_INSERTS: usize = 2;

impl<'tcx> LateLintPass<'tcx> for MapInitThenInsert {
    #[expect(
        clippy::indexing_slicing,
        reason = "indices are bounds-checked by the while condition"
    )]
    fn check_block(&mut self, cx: &LateContext<'tcx>, block: &'tcx Block<'tcx>) {
        let stmts = block.stmts;
        let mut i = 0;

        while i < stmts.len() {
            let Some((binding_id, map_type_name)) = map_init_binding(
                cx,
                &stmts[i],
                self.sym_indexmap_crate,
                self.sym_indexmap_type,
            ) else {
                i += 1;
                continue;
            };

            let insert_start = i + 1;
            let insert_count = count_consecutive_inserts(&stmts[insert_start..], binding_id);

            if insert_count >= MIN_INSERTS {
                let init_span = stmts[i].span;
                let last_insert_span = stmts[insert_start + insert_count - 1].span;
                let full_span = init_span.to(last_insert_span);

                span_lint_and_help(
                    cx,
                    MAP_INIT_THEN_INSERT,
                    full_span,
                    format!(
                        "immediately inserting into a newly created map \
                         \u{2014} consider using `{map_type_name}::from([..])`"
                    ),
                    None,
                    format!(
                        "use `let m = {map_type_name}::from([..])` to initialize the map inline"
                    ),
                );
            }

            // Skip past the insert sequence regardless.
            i = insert_start + insert_count;
        }
    }
}

// ── Helper functions ────────────────────────────────────────────────

/// If `stmt` is `let [mut] <name> = <MapType>::new()` (or `::default()` or
/// `::with_capacity(_)`), returns the binding's `HirId` and a display name
/// for the map type (e.g. `"HashMap"`).
fn map_init_binding<'tcx>(
    cx: &LateContext<'tcx>,
    stmt: &Stmt<'tcx>,
    sym_indexmap_crate: Symbol,
    sym_indexmap_type: Symbol,
) -> Option<(HirId, &'static str)> {
    let StmtKind::Let(local) = &stmt.kind else {
        return None;
    };
    let init = local.init?;

    if stmt.span.from_expansion() {
        return None;
    }

    let ExprKind::Call(callee, _args) = &init.kind else {
        return None;
    };

    let ty = cx.typeck_results().expr_ty(init);
    let type_name = recognized_map_type(cx, ty, sym_indexmap_crate, sym_indexmap_type)?;

    if !is_map_constructor(cx, callee) {
        return None;
    }

    let PatKind::Binding(_, hir_id, _, _) = local.pat.kind else {
        return None;
    };

    Some((hir_id, type_name))
}

/// Returns the display name if `ty` is a recognized map type, `None` otherwise.
///
/// HashMap/BTreeMap use `is_diagnostic_item` (robust, compiler-provided).
/// `IndexMap` uses `crate_name` + `item_name` matching (no diagnostic item
/// exists for third-party crates). Follows the same pattern as
/// `proper_error_type.rs` for anyhow/miette detection.
fn recognized_map_type<'tcx>(
    cx: &LateContext<'tcx>,
    ty: Ty<'tcx>,
    sym_indexmap_crate: Symbol,
    sym_indexmap_type: Symbol,
) -> Option<&'static str> {
    let ty::Adt(adt, _) = ty.kind() else {
        return None;
    };
    let def_id = adt.did();

    if cx.tcx.is_diagnostic_item(sym::HashMap, def_id) {
        Some("HashMap")
    } else if cx.tcx.is_diagnostic_item(sym::BTreeMap, def_id) {
        Some("BTreeMap")
    } else if cx.tcx.crate_name(def_id.krate) == sym_indexmap_crate
        && cx.tcx.item_name(def_id) == sym_indexmap_type
    {
        Some("IndexMap")
    } else {
        None
    }
}

/// Returns `true` if the callee expression resolves to one of the recognized
/// constructors: `new`, `default`, or `with_capacity`.
///
/// Chose name-based matching over `DefId` matching: these are standard
/// constructor names shared across all map types.
fn is_map_constructor(cx: &LateContext<'_>, callee: &Expr<'_>) -> bool {
    let ExprKind::Path(ref qpath) = callee.kind else {
        return false;
    };
    let Some(def_id) = cx.qpath_res(qpath, callee.hir_id).opt_def_id() else {
        return false;
    };

    let name = cx.tcx.item_name(def_id);
    matches!(name.as_str(), "new" | "default" | "with_capacity")
}

/// Counts how many consecutive statements are `.insert(k, v)` calls on the
/// binding identified by `binding_id`. Stops at the first non-insert statement.
fn count_consecutive_inserts(stmts: &[Stmt<'_>], binding_id: HirId) -> usize {
    stmts
        .iter()
        .take_while(|stmt| is_insert_on_binding(stmt, binding_id))
        .count()
}

/// Returns `true` if `stmt` is `<binding>.insert(k, v)` — a semicolon
/// expression statement calling the `insert` method on the given binding.
fn is_insert_on_binding(stmt: &Stmt<'_>, binding_id: HirId) -> bool {
    let StmtKind::Semi(expr) = &stmt.kind else {
        return false;
    };

    if stmt.span.from_expansion() {
        return false;
    }

    // MethodCall fields: (PathSegment, receiver, args, span).
    let ExprKind::MethodCall(method, receiver, args, _) = &expr.kind else {
        return false;
    };

    if method.ident.as_str() != "insert" || args.len() != 2 {
        return false;
    }

    path_to_local_with_projections(receiver) == Some(binding_id)
}

#[cfg(test)]
mod tests {
    use dylint_testing::ui;

    #[test]
    fn ui_map_init_then_insert() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "map_init_then_insert").run();
    }
}

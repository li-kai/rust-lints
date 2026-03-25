#![allow(
    clippy::wildcard_enum_match_arm,
    reason = "only specific variants are relevant"
)]

//! Shared helpers for resolving cross-module references from HIR nodes.

use clippy_utils::is_in_test;
use rustc_hir::def::Res;
use rustc_hir::definitions::DefPathData;
use rustc_hir::{Expr, ExprKind, HirId, Item, ItemKind};
use rustc_lint::{LateContext, LintContext as _};
use rustc_middle::ty::{self, TyCtxt};
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol, sym};

/// Returns `true` if this reference should be skipped by module-level lints
/// (external crate items, macro expansions, test crates, test code).
pub fn should_skip_ref(cx: &LateContext<'_>, def_id: DefId, hir_id: HirId, span: Span) -> bool {
    !def_id.is_local()
        || span.from_expansion()
        || cx.sess().is_test_crate()
        || is_in_test(cx.tcx, hir_id)
}

/// Resolves the `DefId` of the item referenced by an expression, if any.
///
/// Handles path expressions, struct literals, and method calls.
pub fn resolve_expr_def_id(cx: &LateContext<'_>, expr: &Expr<'_>) -> Option<(DefId, HirId, Span)> {
    let qpath_def = |qpath| match cx.qpath_res(qpath, expr.hir_id) {
        Res::Def(_, def_id) => Some(def_id),
        _ => None,
    };
    let def_id = match &expr.kind {
        ExprKind::Path(qpath) => qpath_def(qpath),
        ExprKind::Struct(qpath, _, _) => qpath_def(qpath),
        ExprKind::MethodCall(..) => cx.typeck_results().type_dependent_def_id(expr.hir_id),
        _ => None,
    };
    def_id.map(|id| (id, expr.hir_id, expr.span))
}

/// Resolves the `DefId` of a type reference, if it refers to a named definition.
pub fn resolve_ty_def_id<'tcx>(
    cx: &LateContext<'tcx>,
    ty: &'tcx rustc_hir::Ty<'tcx, rustc_hir::AmbigArg>,
) -> Option<(DefId, HirId, Span)> {
    if let rustc_hir::TyKind::Path(ref qpath) = ty.kind
        && let Res::Def(_, def_id) = cx.qpath_res(qpath, ty.hir_id)
    {
        Some((def_id, ty.hir_id, ty.span))
    } else {
        None
    }
}

/// Yields each `DefId` imported by a `use` item.
pub fn for_each_use_def_id(item: &Item<'_>, mut cb: impl FnMut(DefId, HirId, Span)) {
    if let ItemKind::Use(path, _) = &item.kind {
        for res in path.res.iter().flatten() {
            if let Res::Def(_, def_id) = res {
                cb(*def_id, item.hir_id(), item.span);
            }
        }
    }
}

/// Returns `true` if the receiver type of a method call is `Option` or `Result`.
///
/// Accepts explicit `TypeckResults` because callers in `check_impl_item`
/// callbacks may not have body-level typeck results set on the `LateContext`.
pub fn receiver_is_option_or_result<'tcx>(
    cx: &LateContext<'tcx>,
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    receiver: &Expr<'tcx>,
) -> bool {
    let recv_ty = typeck.expr_ty_adjusted(receiver).peel_refs();
    if let ty::Adt(adt, _) = recv_ty.kind() {
        let did = adt.did();
        return cx.tcx.is_diagnostic_item(sym::Option, did)
            || cx.tcx.is_diagnostic_item(sym::Result, did);
    }
    false
}

/// Returns the named module path components for a definition (e.g. `[payments, checkout]`).
pub fn def_path_segments(tcx: TyCtxt<'_>, def_id: DefId) -> Vec<Symbol> {
    tcx.def_path(def_id)
        .data
        .iter()
        .filter_map(|d| match d.data {
            DefPathData::TypeNs(sym) => Some(sym),
            _ => None,
        })
        .collect()
}

#![allow(
    clippy::wildcard_enum_match_arm,
    reason = "only specific variants are relevant"
)]

//! Shared helpers for resolving cross-module references from HIR nodes.

use clippy_utils::is_in_test;
use clippy_utils::macros::expn_backtrace;
use rustc_hir::def::Res;
use rustc_hir::definitions::DefPathData;
use rustc_hir::{Body, Expr, ExprKind, HirId, Item, ItemKind, Node};
use rustc_lint::{LateContext, LintContext as _};
use rustc_middle::ty::{self, TyCtxt};
use rustc_span::def_id::DefId;
use rustc_span::{ExpnKind, Span, Symbol, sym};

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

/// If `expr` is a panicking `.unwrap()` or `.expect()` on `Option`/`Result`,
/// returns the method-call span and a short description for diagnostics.
pub fn panicking_unwrap_or_expect<'tcx>(
    cx: &LateContext<'tcx>,
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    expr: &Expr<'tcx>,
) -> Option<(Span, &'static str)> {
    let ExprKind::MethodCall(method, receiver, _, span) = &expr.kind else {
        return None;
    };
    let desc = match method.ident.as_str() {
        "unwrap" => ".unwrap()",
        "expect" => ".expect()",
        _ => return None,
    };
    receiver_is_option_or_result(cx, typeck, receiver).then_some((*span, desc))
}

/// If `expr` is a closure that is immediately invoked (e.g. `(|| panic!())()`),
/// returns its body for the caller to walk. Returns `None` for closures stored
/// in a field, passed as a callback, returned, etc. — those don't run eagerly.
pub fn iife_closure_body<'tcx>(tcx: TyCtxt<'tcx>, expr: &Expr<'_>) -> Option<&'tcx Body<'tcx>> {
    let ExprKind::Closure(closure) = expr.kind else {
        return None;
    };
    let is_iife = matches!(
        tcx.parent_hir_node(expr.hir_id),
        Node::Expr(Expr {
            kind: ExprKind::Call(callee, _),
            ..
        }) if callee.hir_id == expr.hir_id
    );
    is_iife.then(|| tcx.hir_body(closure.body))
}

/// Which panic-family macro was detected.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PanicMacro {
    Panic,
    Unreachable,
    Assert,
    AssertEq,
    AssertNe,
}

impl PanicMacro {
    /// Human-readable label for diagnostics.
    pub fn desc(self) -> &'static str {
        match self {
            Self::Panic => "panic!()",
            Self::Unreachable => "unreachable!()",
            Self::Assert => "assert!()",
            Self::AssertEq => "assert_eq!()",
            Self::AssertNe => "assert_ne!()",
        }
    }
}

/// Checks if a span originates from a panic-related macro, walking up the
/// expansion chain to handle cases like `panic!` expanding through internal
/// macros (`panic_fmt`, `panic_2021`, etc.).
///
/// `todo!()` and `unimplemented!()` are deliberately never reported: they mark
/// intentional development placeholders and the compiler already surfaces
/// them. Their with-args forms expand *through* `panic!`, so the whole
/// backtrace is cleared of those frames before any frame is matched —
/// otherwise the inner `panic!` frame would be reported anyway.
pub fn find_panic_macro(span: Span) -> Option<(Span, PanicMacro)> {
    let macro_name = |data: &rustc_span::ExpnData| match data.kind {
        ExpnKind::Macro(_, name) => Some(name),
        _ => None,
    };
    // One pass: `todo!`/`unimplemented!` (incl. with-args forms that expand
    // through `panic!`) suppress the whole chain; otherwise take the first
    // panic-family frame.
    let mut found = None;
    for (_, data) in expn_backtrace(span) {
        let Some(name) = macro_name(&data) else {
            continue;
        };
        match name.as_str() {
            "todo" | "unimplemented" => return None,
            name if found.is_none() => {
                let kind = match name {
                    "panic" => PanicMacro::Panic,
                    "unreachable" => PanicMacro::Unreachable,
                    "assert" => PanicMacro::Assert,
                    "assert_eq" => PanicMacro::AssertEq,
                    "assert_ne" => PanicMacro::AssertNe,
                    _ => continue,
                };
                found = Some((data.call_site, kind));
            }
            _ => {}
        }
    }
    found
}

/// If `ty` is the opaque `impl Future<Output = T>` that an `async fn`
/// returns, extracts `T`. Returns `ty` unchanged otherwise.
///
/// Lints that inspect function return types must peel this, or `async fn`
/// signatures are invisible to them: `clippy_utils::return_ty` yields the
/// opaque future type, never the `Output` the source code spells out.
pub fn peel_async_fn_return_ty<'tcx>(tcx: TyCtxt<'tcx>, ty: ty::Ty<'tcx>) -> ty::Ty<'tcx> {
    let ty::Alias(ty::Opaque, alias) = ty.kind() else {
        return ty;
    };
    let Some(future_output) = tcx.lang_items().future_output() else {
        return ty;
    };
    tcx.explicit_item_bounds(alias.def_id)
        .iter_instantiated_copied(tcx, alias.args)
        .find_map(|(clause, _)| {
            if let ty::ClauseKind::Projection(proj) = clause.kind().skip_binder()
                && proj.projection_term.def_id == future_output
            {
                proj.term.as_type()
            } else {
                None
            }
        })
        .unwrap_or(ty)
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

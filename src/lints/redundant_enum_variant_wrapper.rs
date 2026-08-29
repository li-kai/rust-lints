#![allow(
    clippy::wildcard_enum_match_arm,
    reason = "only constructor-related HIR expression variants are relevant"
)]

use clippy_utils::diagnostics::span_lint_hir_and_then;
use clippy_utils::res::MaybeResPath as _;
use clippy_utils::ty::adt_and_variant_of_res;
use rustc_hir::{
    Body, Expr, ExprKind, HirId, ImplItem, ImplItemKind, Item, ItemKind, PatKind, QPath,
    StructTailExpr,
};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty;
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol};

use super::constructor;

rustc_session::declare_lint! {
    /// Flags associated functions that only forward parameters to an enum
    /// variant when the enum has no nontrivial constructors.
    pub REDUNDANT_ENUM_VARIANT_WRAPPER,
    Deny,
    "associated function redundantly wraps an enum variant constructor"
}

struct DirectWrapper {
    variant_def_id: DefId,
    hir_id: HirId,
    span: Span,
    function_name: Symbol,
}

/// Removes expression-only blocks and explicit returns around a value.
fn peel_transparent_exprs<'tcx>(mut expr: &'tcx Expr<'tcx>) -> &'tcx Expr<'tcx> {
    loop {
        match expr.kind {
            ExprKind::Block(block, _) if block.stmts.is_empty() => {
                let Some(tail) = block.expr else {
                    return expr;
                };
                expr = tail;
            }
            ExprKind::Ret(Some(value)) => expr = value,
            _ => return expr,
        }
    }
}

fn simple_parameter_ids(body: &Body<'_>) -> Option<Vec<HirId>> {
    body.params
        .iter()
        .map(|param| match param.pat.kind {
            PatKind::Binding(_, id, _, None) => Some(id),
            _ => None,
        })
        .collect()
}

fn direct_parameter_id(typeck: &ty::TypeckResults<'_>, expr: &Expr<'_>) -> Option<HirId> {
    let expr = peel_transparent_exprs(expr);
    if !typeck.expr_adjustments(expr).is_empty() {
        return None;
    }
    expr.res_local_id()
}

fn resolved_variant(
    cx: &LateContext<'_>,
    typeck: &ty::TypeckResults<'_>,
    qpath: &QPath<'_>,
    hir_id: HirId,
) -> Option<(DefId, DefId)> {
    let (adt, variant) = adt_and_variant_of_res(cx, typeck.qpath_res(qpath, hir_id))?;
    adt.is_enum().then_some((adt.did(), variant.def_id))
}

fn forwards_each_parameter_once<'tcx>(
    typeck: &ty::TypeckResults<'tcx>,
    mut unmatched: Vec<HirId>,
    mut values: impl ExactSizeIterator<Item = &'tcx Expr<'tcx>>,
) -> bool {
    if unmatched.len() != values.len() {
        return false;
    }

    let mut remove_parameter = |value| {
        let Some(id) = direct_parameter_id(typeck, value) else {
            return false;
        };
        let Some(index) = unmatched.iter().position(|candidate| *candidate == id) else {
            return false;
        };
        unmatched.swap_remove(index);
        true
    };

    values.all(&mut remove_parameter)
}

/// Returns the enum and variant definitions when the expression directly
/// forwards every parameter to one variant.
fn directly_constructed_variant<'tcx>(
    cx: &LateContext<'tcx>,
    typeck: &ty::TypeckResults<'tcx>,
    expr: &'tcx Expr<'tcx>,
    parameter_ids: Vec<HirId>,
) -> Option<(DefId, DefId)> {
    match &expr.kind {
        ExprKind::Call(callee, args) => {
            let ExprKind::Path(qpath) = &callee.kind else {
                return None;
            };
            let variant = resolved_variant(cx, typeck, qpath, callee.hir_id)?;
            forwards_each_parameter_once(typeck, parameter_ids, args.iter()).then_some(variant)
        }
        ExprKind::Struct(qpath, fields, StructTailExpr::None) => {
            let variant = resolved_variant(cx, typeck, qpath, expr.hir_id)?;
            forwards_each_parameter_once(
                typeck,
                parameter_ids,
                fields.iter().map(|field| field.expr),
            )
            .then_some(variant)
        }
        ExprKind::Path(qpath) if parameter_ids.is_empty() => {
            resolved_variant(cx, typeck, qpath, expr.hir_id)
        }
        _ => None,
    }
}

fn direct_wrapper<'tcx>(
    cx: &LateContext<'tcx>,
    enum_def_id: DefId,
    item: &'tcx ImplItem<'tcx>,
) -> Option<DirectWrapper> {
    let ImplItemKind::Fn(_, body_id) = item.kind else {
        return None;
    };
    let body = cx.tcx.hir_body(body_id);
    let constructor = peel_transparent_exprs(body.value);

    // Preserve macro-generated constructors as an enum-wide exemption rather
    // than treating expansion provenance as a reportable direct wrapper.
    if item.span.from_expansion() || constructor.span.from_expansion() {
        return None;
    }

    let typeck = cx.tcx.typeck(item.owner_id.def_id);
    let Some(parameter_ids) = simple_parameter_ids(body) else {
        return None;
    };
    let Some((constructed_adt_def_id, variant_def_id)) =
        directly_constructed_variant(cx, typeck, constructor, parameter_ids)
    else {
        return None;
    };
    if constructed_adt_def_id != enum_def_id {
        return None;
    }

    Some(DirectWrapper {
        variant_def_id,
        hir_id: item.hir_id(),
        span: item.span,
        function_name: item.ident.name,
    })
}

pub struct RedundantEnumVariantWrapper;

impl RedundantEnumVariantWrapper {
    pub const fn new() -> Self {
        Self
    }
}

rustc_session::impl_lint_pass!(RedundantEnumVariantWrapper => [REDUNDANT_ENUM_VARIANT_WRAPPER]);

impl<'tcx> LateLintPass<'tcx> for RedundantEnumVariantWrapper {
    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        let ItemKind::Enum(..) = item.kind else {
            return;
        };
        let enum_def_id = item.owner_id.to_def_id();
        let mut wrappers = Vec::new();

        for impl_def_id in cx.tcx.inherent_impls(enum_def_id) {
            for assoc in cx.tcx.associated_items(*impl_def_id).in_definition_order() {
                let ty::AssocKind::Fn {
                    has_self: false, ..
                } = assoc.kind
                else {
                    continue;
                };
                if constructor::return_adt(cx.tcx, assoc.def_id)
                    .is_none_or(|adt| adt.did() != enum_def_id)
                {
                    continue;
                }
                let Some(local_def_id) = assoc.def_id.as_local() else {
                    continue;
                };
                let impl_item = cx.tcx.hir_expect_impl_item(local_def_id);
                let Some(wrapper) = direct_wrapper(cx, enum_def_id, impl_item) else {
                    return;
                };
                wrappers.push(wrapper);
            }
        }

        wrappers.sort_by_key(|wrapper| wrapper.span.lo());
        let enum_name = cx.tcx.item_name(enum_def_id);
        for wrapper in wrappers {
            let variant_name = cx.tcx.item_name(wrapper.variant_def_id);
            span_lint_hir_and_then(
                cx,
                REDUNDANT_ENUM_VARIANT_WRAPPER,
                wrapper.hir_id,
                wrapper.span,
                format!(
                    "associated function `{}` only wraps enum variant `{variant_name}`",
                    wrapper.function_name
                ),
                |diag| {
                    diag.help(format!("construct `{enum_name}::{variant_name}` directly"));
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ui_redundant_enum_variant_wrapper() {
        crate::testing::run_ui_test("redundant_enum_variant_wrapper", None, &[]);
    }
}

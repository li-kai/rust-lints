#![allow(
    clippy::wildcard_enum_match_arm,
    reason = "only Result and Box return types are unwrapped"
)]

//! Shared classification of conventional constructor return types.

use rustc_hir::LangItem;
use rustc_middle::ty::{self, TyCtxt};
use rustc_span::def_id::DefId;
use rustc_span::sym;

/// Returns the function's return ADT after peeling one supported constructor
/// wrapper: `Result<_, _>` or `Box<_>`.
pub fn return_adt(tcx: TyCtxt<'_>, function_def_id: DefId) -> Option<ty::AdtDef<'_>> {
    let ret_ty = tcx
        .fn_sig(function_def_id)
        .instantiate_identity()
        .output()
        .skip_binder();
    let inner_ty = match ret_ty.kind() {
        ty::Adt(adt, args)
            if tcx.is_diagnostic_item(sym::Result, adt.did())
                || tcx.is_lang_item(adt.did(), LangItem::OwnedBox) =>
        {
            args.type_at(0)
        }
        _ => ret_ty,
    };
    inner_ty.ty_adt_def()
}

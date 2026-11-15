//! Type queries read without normalization.
//!
//! On this toolchain `type_of`, `fn_sig` and `FieldDef::ty` return
//! `ty::Unnormalized`. The lints only inspect the shape of a type (its ADT,
//! its trait impls), so they skip normalization. These wrappers keep the
//! work-in-progress `skip_norm_wip` API in one place. The only other caller is
//! `hir_refs::peel_async_fn_return_ty`, which reads item bounds.

use rustc_middle::ty::{self, GenericArgsRef, Ty, TyCtxt};
use rustc_span::def_id::DefId;

/// The type of `def_id` with its own generic parameters left in place.
pub fn type_of(tcx: TyCtxt<'_>, def_id: impl Into<DefId>) -> Ty<'_> {
    tcx.type_of(def_id.into())
        .instantiate_identity()
        .skip_norm_wip()
}

/// The signature of `def_id` with its own generic parameters left in place.
pub fn fn_sig(tcx: TyCtxt<'_>, def_id: impl Into<DefId>) -> ty::PolyFnSig<'_> {
    tcx.fn_sig(def_id.into())
        .instantiate_identity()
        .skip_norm_wip()
}

/// The type of `field` with `args` substituted for its ADT's parameters.
pub fn field_ty<'tcx>(
    tcx: TyCtxt<'tcx>,
    field: &ty::FieldDef,
    args: GenericArgsRef<'tcx>,
) -> Ty<'tcx> {
    field.ty(tcx, args).skip_norm_wip()
}

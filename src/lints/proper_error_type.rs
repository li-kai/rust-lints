use clippy_utils::diagnostics::span_lint_and_help;
use clippy_utils::ty::implements_trait;
use clippy_utils::visitors::for_each_expr_without_closures;
use clippy_utils::{is_def_id_trait_method, is_entrypoint_fn, is_in_cfg_test, return_ty};
use core::ops::ControlFlow;
use rustc_data_structures::fx::FxHashMap;
use rustc_hir::intravisit::FnKind;
use rustc_hir::{Body, ExprKind, FnDecl, Item, ItemKind, LangItem};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty::{self, ExistentialPredicate, Ty};
use rustc_span::{Span, Symbol, sym};

rustc_session::declare_lint! {
    /// Flags error types in public APIs that are incomplete, unstructured,
    /// or missing error chain information.
    pub PROPER_ERROR_TYPE,
    Warn,
    "flags improper error types in public API signatures"
}

pub struct ProperErrorType {
    /// `impl Error for T` blocks seen, keyed by the self type's ADT `DefId`.
    error_impls: FxHashMap<rustc_hir::def_id::DefId, ErrorImplInfo>,
    /// `impl Display for T` blocks seen, keyed by the self type's ADT `DefId`.
    display_impls: FxHashMap<rustc_hir::def_id::DefId, DisplayImplInfo>,
    /// Cached interned symbols.
    sym_source: Symbol,
    sym_fmt: Symbol,
    sym_anyhow: Symbol,
    sym_anyhow_error: Symbol,
    sym_miette: Symbol,
    sym_miette_report: Symbol,
}

impl Default for ProperErrorType {
    fn default() -> Self {
        Self {
            error_impls: FxHashMap::default(),
            display_impls: FxHashMap::default(),
            sym_source: Symbol::intern("source"),
            sym_fmt: Symbol::intern("fmt"),
            sym_anyhow: Symbol::intern("anyhow"),
            sym_anyhow_error: Symbol::intern("Error"),
            sym_miette: Symbol::intern("miette"),
            sym_miette_report: Symbol::intern("Report"),
        }
    }
}

struct ErrorImplInfo {
    span: Span,
    has_source: bool,
    /// Fields of self type that implement Error (for step 3 cross-ref).
    source_field_names: Vec<Symbol>,
}

struct DisplayImplInfo {
    span: Span,
    /// `LocalDefId` of the `fmt` method, for body inspection in step 3.
    fmt_def_id: Option<rustc_hir::def_id::LocalDefId>,
}

rustc_session::impl_lint_pass!(ProperErrorType => [PROPER_ERROR_TYPE]);

/// Describes which kind of unstructured error was detected.
enum UnstructuredKind {
    /// String, &str, Cow<str>, Box<dyn Error>
    Basic(&'static str),
    /// `anyhow::Error` or `miette::Report`
    ErasedCrate(&'static str),
}

impl<'tcx> LateLintPass<'tcx> for ProperErrorType {
    // ── Step 1: Unstructured error types ──
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        kind: FnKind<'tcx>,
        decl: &'tcx FnDecl<'tcx>,
        _body: &'tcx Body<'tcx>,
        span: Span,
        def_id: rustc_hir::def_id::LocalDefId,
    ) {
        // Only lint public, non-test, named functions that aren't macro-generated,
        // trait method implementations, or binary entry points.
        if span.from_expansion()
            || is_def_id_trait_method(cx, def_id)
            || is_entrypoint_fn(cx, def_id.to_def_id())
            || !cx.tcx.visibility(def_id.to_def_id()).is_public()
            || is_in_cfg_test(cx.tcx, cx.tcx.local_def_id_to_hir_id(def_id))
            || matches!(kind, FnKind::Closure)
        {
            return;
        }

        let ret_ty = return_ty(cx, rustc_hir::OwnerId { def_id });
        let ty::Adt(adt, args) = ret_ty.kind() else {
            return;
        };
        if !cx.tcx.is_diagnostic_item(sym::Result, adt.did()) {
            return;
        }
        let err_ty = args.type_at(1);

        let Some(kind) = self.classify_unstructured(cx, err_ty) else {
            return;
        };

        let ret_span = decl.output.span();

        match kind {
            UnstructuredKind::Basic(name) => {
                span_lint_and_help(
                    cx,
                    PROPER_ERROR_TYPE,
                    ret_span,
                    format!(
                        "public function returns `Result<_, {name}>` — use a type that implements `Error`"
                    ),
                    None,
                    "define an error enum with `#[derive(thiserror::Error)]`",
                );
            }
            UnstructuredKind::ErasedCrate(crate_name) => {
                // Only flag if effectively public
                if !cx.tcx.effective_visibilities(()).is_reachable(def_id) {
                    return;
                }
                span_lint_and_help(
                    cx,
                    PROPER_ERROR_TYPE,
                    ret_span,
                    format!(
                        "effectively public function returns `{crate_name}::Error` — use a typed error"
                    ),
                    None,
                    "define an error enum with `#[derive(thiserror::Error)]` for library API surfaces",
                );
            }
        }
    }

    // ── Steps 2, 4, 5: check_item ──
    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        // Step 5: *Error without Error impl
        Self::check_error_named_type(cx, item);

        // Steps 2-4: collect impl Error / impl Display
        self.collect_trait_impls(cx, item);
    }

    // ── Steps 3, 4: check_crate_post ──
    fn check_crate_post(&mut self, cx: &LateContext<'tcx>) {
        for (adt_did, error_info) in &self.error_impls {
            // Step 4: Manual Error + Display
            if let Some(display_info) = self.display_impls.get(adt_did) {
                span_lint_and_help(
                    cx,
                    PROPER_ERROR_TYPE,
                    error_info.span,
                    "manual `Error` + `Display` impl \u{2014} use `#[derive(thiserror::Error)]`",
                    None,
                    "thiserror eliminates boilerplate and keeps Display in sync with variants",
                );

                // Step 3: Duplicated source in Display
                if error_info.has_source
                    && !error_info.source_field_names.is_empty()
                    && let Some(fmt_def_id) = display_info.fmt_def_id
                {
                    Self::check_duplicated_source(
                        cx,
                        fmt_def_id,
                        &error_info.source_field_names,
                        display_info.span,
                    );
                }
            }
        }
    }
}

impl ProperErrorType {
    /// Check if `err_ty` is one of the unstructured error patterns.
    fn classify_unstructured<'tcx>(
        &self,
        cx: &LateContext<'tcx>,
        err_ty: Ty<'tcx>,
    ) -> Option<UnstructuredKind> {
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "ty::TyKind has too many variants to enumerate; all unrecognised kinds correctly return None"
        )]
        match err_ty.kind() {
            // &str
            ty::Ref(_, inner, _) if inner.is_str() => Some(UnstructuredKind::Basic("&str")),
            ty::Adt(adt, args) => {
                let did = adt.did();
                if cx.tcx.is_lang_item(did, LangItem::String) {
                    return Some(UnstructuredKind::Basic("String"));
                }
                if cx.tcx.is_diagnostic_item(sym::Cow, did) && args.type_at(0).is_str() {
                    return Some(UnstructuredKind::Basic("Cow<'_, str>"));
                }
                if cx.tcx.is_lang_item(did, LangItem::OwnedBox)
                    && let ty::Dynamic(preds, ..) = args.type_at(0).kind()
                    && let Some(error_trait_id) = cx.tcx.get_diagnostic_item(sym::Error)
                    && preds.iter().any(|pred| {
                        matches!(pred.skip_binder(), ExistentialPredicate::Trait(t) if t.def_id == error_trait_id)
                    })
                {
                    return Some(UnstructuredKind::Basic("Box<dyn Error>"));
                }
                let crate_name = cx.tcx.crate_name(did.krate);
                let item_name = cx.tcx.item_name(did);
                if crate_name == self.sym_anyhow && item_name == self.sym_anyhow_error {
                    Some(UnstructuredKind::ErasedCrate("anyhow"))
                } else if crate_name == self.sym_miette && item_name == self.sym_miette_report {
                    Some(UnstructuredKind::ErasedCrate("miette"))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Step 5: Check if a struct/enum named `*Error` or `*Err` implements `Error`.
    fn check_error_named_type<'tcx>(cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "ItemKind has many variants; all non-ADT kinds are correctly skipped"
        )]
        let ident = match &item.kind {
            ItemKind::Struct(ident, _, _) | ItemKind::Enum(ident, _, _) => *ident,
            _ => return,
        };

        let name = ident.as_str();
        if item.span.from_expansion() || (!name.ends_with("Error") && !name.ends_with("Err")) {
            return;
        }

        // Only lint public types outside of test code; private types don't form
        // part of the library's API surface.
        if !cx.tcx.visibility(item.owner_id.to_def_id()).is_public()
            || is_in_cfg_test(cx.tcx, cx.tcx.local_def_id_to_hir_id(item.owner_id.def_id))
        {
            return;
        }

        let ty = cx.tcx.type_of(item.owner_id.def_id).instantiate_identity();
        let Some(error_trait_id) = cx.tcx.get_diagnostic_item(sym::Error) else {
            return;
        };
        if !implements_trait(cx, ty, error_trait_id, &[]) {
            span_lint_and_help(
                cx,
                PROPER_ERROR_TYPE,
                item.span,
                format!(
                    "`{name}` is named as an error type but does not implement `std::error::Error`"
                ),
                None,
                "add `#[derive(thiserror::Error, Debug)]`",
            );
        }
    }

    /// Collect `impl Error for T` and `impl Display for T` blocks.
    fn collect_trait_impls<'tcx>(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        let ItemKind::Impl(impl_) = &item.kind else {
            return;
        };
        if item.span.from_expansion() {
            return;
        }
        let Some(trait_header) = impl_.of_trait else {
            return;
        };
        let Some(trait_def_id) = trait_header.trait_ref.trait_def_id() else {
            return;
        };

        let self_ty = cx.tcx.type_of(item.owner_id.def_id).instantiate_identity();
        let Some(adt_def) = self_ty.ty_adt_def() else {
            return;
        };
        let adt_did = adt_def.did();

        let error_trait_id = cx.tcx.get_diagnostic_item(sym::Error);
        let display_trait_id = cx.tcx.get_diagnostic_item(sym::Display);

        // impl Error for T
        if Some(trait_def_id) == error_trait_id {
            let has_source = impl_.items.iter().any(|impl_item_ref| {
                let impl_item = cx.tcx.hir_impl_item(*impl_item_ref);
                impl_item.ident.name == self.sym_source
            });

            // Find fields that implement Error
            let mut source_field_names = Vec::new();
            if let (Some(error_tid), ty::Adt(_, args)) = (error_trait_id, self_ty.kind()) {
                for variant in adt_def.variants() {
                    for field in &variant.fields {
                        let field_ty = field.ty(cx.tcx, args);
                        if implements_trait(cx, field_ty, error_tid, &[]) {
                            source_field_names.push(field.name);
                        }
                    }
                }
            }

            // Step 2: Missing source() — emit immediately
            if !has_source && !source_field_names.is_empty() {
                let type_name = cx.tcx.item_name(adt_did);
                span_lint_and_help(
                    cx,
                    PROPER_ERROR_TYPE,
                    item.span,
                    format!("`{type_name}` wraps error types but does not implement `source()`"),
                    None,
                    "use `#[derive(thiserror::Error)]` with `#[source]` / `#[from]`",
                );
            }

            self.error_impls.insert(
                adt_did,
                ErrorImplInfo {
                    span: item.span,
                    has_source,
                    source_field_names,
                },
            );
        }

        // impl Display for T
        if Some(trait_def_id) == display_trait_id {
            let fmt_def_id = impl_.items.iter().find_map(|impl_item_ref| {
                let impl_item = cx.tcx.hir_impl_item(*impl_item_ref);
                (impl_item.ident.name == self.sym_fmt).then_some(impl_item.owner_id.def_id)
            });

            self.display_impls.insert(
                adt_did,
                DisplayImplInfo {
                    span: item.span,
                    fmt_def_id,
                },
            );
        }
    }

    /// Step 3: Check if Display fmt body references error-typed fields.
    fn check_duplicated_source(
        cx: &LateContext<'_>,
        fmt_def_id: rustc_hir::def_id::LocalDefId,
        source_field_names: &[Symbol],
        display_span: Span,
    ) {
        let Some(error_trait_id) = cx.tcx.get_diagnostic_item(sym::Error) else {
            return;
        };
        let body = cx.tcx.hir_body_owned_by(fmt_def_id);
        let typeck = cx.tcx.typeck(fmt_def_id);

        let found = for_each_expr_without_closures(body, |expr| {
            // Check field access by name (struct-style fields)
            if let ExprKind::Field(_, ident) = expr.kind
                && source_field_names.contains(&ident.name)
            {
                return ControlFlow::Break(());
            }
            // Check if any expression whose type implements Error is used.
            // This catches pattern-bound variables from enum tuple variants
            // (e.g. `Self::Io(e) => write!(f, "{e}")`)
            if let Some(expr_ty) = typeck.node_type_opt(expr.hir_id)
                && implements_trait(cx, expr_ty.peel_refs(), error_trait_id, &[])
                && !matches!(expr.kind, ExprKind::Path(rustc_hir::QPath::Resolved(_, path))
                    if path.segments.last().is_some_and(|s| s.ident.name == rustc_span::symbol::kw::SelfLower))
            {
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        });

        if found.is_some() {
            span_lint_and_help(
                cx,
                PROPER_ERROR_TYPE,
                display_span,
                "`Display` renders inner error that is also returned by `source()`",
                None,
                "describe only what went wrong at this level \u{2014} the source chain handles the rest",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use dylint_testing::ui;

    #[test]
    fn ui_proper_error_type() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "proper_error_type").run();
    }
}

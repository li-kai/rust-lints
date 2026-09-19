use std::collections::VecDeque;

use clippy_utils::diagnostics::span_lint_hir_and_then;
use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_hir::attrs::lang_items::LangItem;
use rustc_hir::{ExprKind, ImplItemKind, Item, ItemKind, Safety};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty::{self, Ty};
use rustc_span::{Span, Symbol, sym};

use super::call_matching::path_final_segment;
use crate::config::UnsafeSendThreadAffineDropConfig;

rustc_lint::declare_lint! {
    /// Warns when `unsafe impl Send` permits a declared thread-affine value
    /// to be destroyed by ordinary drop glue on another thread.
    pub UNSAFE_SEND_THREAD_AFFINE_DROP,
    Warn,
    "`unsafe impl Send` contains thread-affine state in automatic drop glue"
}

struct ThreadAffineContract {
    path: String,
    reason: String,
}

struct Hazard {
    span: Span,
    field_name: Symbol,
    contract_path: String,
    reason: String,
    ownership_path: Vec<String>,
    is_self: bool,
}

pub struct UnsafeSendThreadAffineDrop {
    external_contracts: FxHashMap<String, ThreadAffineContract>,
    external_type_names: FxHashSet<Symbol>,
    contract_marker: Symbol,
}

impl UnsafeSendThreadAffineDrop {
    pub fn new() -> Self {
        let config: UnsafeSendThreadAffineDropConfig =
            dylint_linting::config_or_default("unsafe_send_thread_affine_drop");

        let mut external_contracts = FxHashMap::default();
        let mut external_type_names = FxHashSet::default();
        for configured in config.external_types {
            external_type_names.insert(path_final_segment(&configured.path));
            external_contracts.insert(
                configured.path.clone(),
                ThreadAffineContract {
                    path: configured.path,
                    reason: configured.reason,
                },
            );
        }

        Self {
            external_contracts,
            external_type_names,
            contract_marker: Symbol::intern("__RUST_LINTS_THREAD_AFFINE_DROP_CONTRACT"),
        }
    }

    fn local_contract(
        &self,
        cx: &LateContext<'_>,
        adt: ty::AdtDef<'_>,
    ) -> Option<ThreadAffineContract> {
        if !adt.did().is_local() {
            return None;
        }

        for impl_def_id in cx.tcx.inherent_impls(adt.did()) {
            for assoc in cx.tcx.associated_items(*impl_def_id).in_definition_order() {
                if assoc.name() != self.contract_marker
                    || !matches!(assoc.kind, ty::AssocKind::Const { .. })
                {
                    continue;
                }
                let local_def_id = assoc.def_id.as_local()?;
                let impl_item = cx.tcx.hir_expect_impl_item(local_def_id);
                let ImplItemKind::Const(_, rustc_hir::ConstItemRhs::Body(body_id)) = impl_item.kind
                else {
                    continue;
                };
                let body = cx.tcx.hir_body(body_id);
                let ExprKind::Lit(literal) = body.value.kind else {
                    continue;
                };
                let rustc_ast::LitKind::Str(reason, _) = literal.node else {
                    continue;
                };
                let local_path = cx.tcx.def_path_str(adt.did());
                return Some(ThreadAffineContract {
                    path: format!("crate::{local_path}"),
                    reason: reason.as_str().to_owned(),
                });
            }
        }
        None
    }

    fn contract_for_adt(
        &self,
        cx: &LateContext<'_>,
        adt: ty::AdtDef<'_>,
    ) -> Option<ThreadAffineContract> {
        if let Some(contract) = self.local_contract(cx, adt) {
            return Some(contract);
        }
        if adt.did().is_local() {
            return None;
        }

        self.external_type_names
            .contains(&cx.tcx.item_name(adt.did()))
            .then(|| cx.tcx.def_path_str(adt.did()))
            .and_then(|path| self.external_contracts.get(&path))
            .map(|contract| ThreadAffineContract {
                path: contract.path.clone(),
                reason: contract.reason.clone(),
            })
    }

    fn is_lang_item(cx: &LateContext<'_>, adt: ty::AdtDef<'_>, item: LangItem) -> bool {
        cx.tcx.is_lang_item(adt.did(), item)
    }

    /// Standard containers whose ownership of their generic arguments is not
    /// represented by ordinary Rust fields (they use raw pointers internally).
    fn is_builtin_owning_container(cx: &LateContext<'_>, adt: ty::AdtDef<'_>) -> bool {
        matches!(
            cx.tcx.def_path_str(adt.did()).as_str(),
            "alloc::boxed::Box"
                | "alloc::vec::Vec"
                | "alloc::rc::Rc"
                | "alloc::sync::Arc"
                | "alloc::collections::binary_heap::BinaryHeap"
                | "alloc::collections::btree::map::BTreeMap"
                | "alloc::collections::btree::set::BTreeSet"
                | "alloc::collections::linked_list::LinkedList"
                | "alloc::collections::vec_deque::VecDeque"
                | "std::boxed::Box"
                | "std::vec::Vec"
                | "std::rc::Rc"
                | "std::sync::Arc"
                | "std::collections::binary_heap::BinaryHeap"
                | "std::collections::btree::map::BTreeMap"
                | "std::collections::btree::set::BTreeSet"
                | "std::collections::linked_list::LinkedList"
                | "std::collections::vec_deque::VecDeque"
                | "std::collections::hash::map::HashMap"
                | "std::collections::hash::set::HashSet"
        )
    }

    /// Finds a declared type reached by ordinary ownership and drop glue.
    ///
    /// `ManuallyDrop` is the deliberate structural boundary: unlike an outer
    /// `Drop` implementation, it actually removes its value from automatic
    /// field drop glue. References, raw pointers, and `PhantomData` do not own
    /// a value to destroy and therefore stop traversal as well.
    fn find_thread_affine_type<'tcx>(
        &self,
        cx: &LateContext<'tcx>,
        ty: Ty<'tcx>,
    ) -> Option<(String, String, Vec<String>)> {
        // Instantiated types can expand forever without repeating, e.g.
        // Node<T> -> Node<Vec<T>>. Bound depth and total examined ownership
        // edges per field, including duplicates, to also bound breadth.
        const MAX_DEPTH: usize = 64;
        const MAX_EDGES: usize = 4096;
        let mut pending = VecDeque::from([(ty, 0, Vec::<Symbol>::new())]);
        let mut visited = FxHashSet::default();
        visited.insert(ty);
        let mut remaining_edges = MAX_EDGES;

        while let Some((ty, depth, mut ownership_path)) = pending.pop_front() {
            if let ty::Adt(adt, _) = ty.kind() {
                if Self::is_lang_item(cx, *adt, LangItem::ManuallyDrop)
                    || Self::is_lang_item(cx, *adt, LangItem::PhantomData)
                    || adt.is_union()
                {
                    continue;
                }
                if let Some(contract) = self.contract_for_adt(cx, *adt) {
                    let ownership_path = ownership_path
                        .into_iter()
                        .map(|name| name.as_str().to_owned())
                        .collect();
                    return Some((contract.path, contract.reason, ownership_path));
                }
            }
            // Exhaustion leaves branches unexplored; it does not prove safety.
            // Still check contracts on all types already queued.
            if depth == MAX_DEPTH || remaining_edges == 0 {
                continue;
            }
            if let ty::Adt(adt, _) = ty.kind() {
                ownership_path.push(cx.tcx.item_name(adt.did()));
            }
            for child in Self::owned_types(cx, ty).take(remaining_edges) {
                remaining_edges -= 1;
                // BFS reaches each type at its shallowest depth first.
                if visited.insert(child) {
                    pending.push_back((child, depth + 1, ownership_path.clone()));
                }
            }
        }
        None
    }

    /// Immediate ownership edges, evaluated lazily so the work budget also
    /// bounds field substitution for wide types.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "unlisted forms do not own a value or cannot prove a concrete declared type"
    )]
    fn owned_types<'a, 'tcx: 'a>(
        cx: &'a LateContext<'tcx>,
        ty: Ty<'tcx>,
    ) -> Box<dyn Iterator<Item = Ty<'tcx>> + 'a> {
        match ty.kind() {
            // These containers own values behind raw pointers. Model contents
            // directly instead of walking their implementation fields.
            ty::Adt(adt, args) if Self::is_builtin_owning_container(cx, *adt) => {
                Box::new(args.types())
            }
            ty::Adt(adt, args) => Box::new(
                adt.all_fields()
                    .map(move |field| field.ty(cx.tcx, args).skip_norm_wip()),
            ),
            ty::Array(element, _) | ty::Slice(element) => Box::new(std::iter::once(*element)),
            ty::Tuple(elements) => Box::new(elements.iter()),
            _ => Box::new(std::iter::empty()),
        }
    }
}

rustc_lint::impl_lint_pass!(UnsafeSendThreadAffineDrop => [UNSAFE_SEND_THREAD_AFFINE_DROP]);

impl<'tcx> LateLintPass<'tcx> for UnsafeSendThreadAffineDrop {
    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        if item.span.from_expansion() {
            return;
        }
        let ItemKind::Impl(impl_block) = &item.kind else {
            return;
        };
        let Some(trait_header) = impl_block.of_trait else {
            return;
        };
        if trait_header.safety != Safety::Unsafe {
            return;
        }
        let Some(trait_def_id) = trait_header.trait_ref.trait_def_id() else {
            return;
        };
        let Some(send_trait_id) = cx.tcx.get_diagnostic_item(sym::Send) else {
            return;
        };
        if trait_def_id != send_trait_id {
            return;
        }

        let self_ty = cx
            .tcx
            .type_of(item.owner_id.def_id)
            .instantiate_identity()
            .skip_norm_wip();
        let ty::Adt(adt, args) = self_ty.kind() else {
            return;
        };

        let type_name = cx.tcx.item_name(adt.did());
        let mut hazards = Vec::new();

        // Declaring the `Send` type itself means the auto-trait escape
        // directly contradicts that type's destruction contract.
        if let Some(contract) = self.contract_for_adt(cx, *adt) {
            hazards.push(Hazard {
                span: cx.tcx.def_span(adt.did()),
                field_name: type_name,
                contract_path: contract.path,
                reason: contract.reason,
                ownership_path: Vec::new(),
                is_self: true,
            });
        } else {
            for variant in adt.variants() {
                for field in &variant.fields {
                    let field_ty = field.ty(cx.tcx, args).skip_norm_wip();
                    if let Some((contract_path, reason, ownership_path)) =
                        self.find_thread_affine_type(cx, field_ty)
                    {
                        hazards.push(Hazard {
                            span: cx.tcx.def_span(field.did),
                            field_name: field.name,
                            contract_path,
                            reason,
                            ownership_path,
                            is_self: false,
                        });
                    }
                }
            }
        }

        if hazards.is_empty() {
            return;
        }

        span_lint_hir_and_then(
            cx,
            UNSAFE_SEND_THREAD_AFFINE_DROP,
            item.hir_id(),
            item.span,
            format!(
                "`unsafe impl Send for {type_name}` permits thread-affine state to be dropped on another thread"
            ),
            |diag| {
                for hazard in &hazards {
                    if hazard.is_self {
                        diag.span_label(
                            hazard.span,
                            format!("declared thread-affine: {}", hazard.reason),
                        );
                    } else {
                        let through = if hazard.ownership_path.is_empty() {
                            String::new()
                        } else {
                            format!(" through `{}`", hazard.ownership_path.join(" → "))
                        };
                        diag.span_label(
                            hazard.span,
                            format!(
                                "`{}` owns `{}`{through}: {}",
                                hazard.field_name, hazard.contract_path, hazard.reason
                            ),
                        );
                    }
                }
                diag.note(
                    "an outer `Drop` implementation does not suppress automatic field drop glue",
                );
                diag.help(
                    "remove the `unsafe impl Send`, or store the thread-affine value behind \
                     `ManuallyDrop` and explicitly destroy it in the required context",
                );
            },
        );
    }
}

#[cfg(test)]
mod tests {
    const TOML: &str = r#"
[unsafe_send_thread_affine_drop]
external_types = [
    { path = "std::path::PathBuf", reason = "synthetic external contract for the UI test" },
]
"#;

    #[test]
    fn ui_unsafe_send_thread_affine_drop() {
        crate::testing::run_ui_test("unsafe_send_thread_affine_drop", Some(TOML), &[]);
    }
}

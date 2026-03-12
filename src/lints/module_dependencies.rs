use clippy_utils::diagnostics::span_lint_and_help;
use clippy_utils::is_in_test;
use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_hir::def::Res;
use rustc_hir::definitions::DefPathData;
use rustc_hir::{Expr, ExprKind, HirId, Item, ItemKind};
use rustc_lint::{LateContext, LateLintPass, LintContext as _};
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol};

use crate::config::ModuleDependenciesConfig;

// ── Lint declarations ────────────────────────────────────────────────

rustc_session::declare_lint! {
    /// Flags cross-module dependencies not declared in the allowlist.
    ///
    /// Each top-level module declares which other top-level modules it may
    /// depend on via `[module_dependencies.allow]` in `dylint.toml`. Any
    /// reference to an item in an undeclared module is a compile-time error.
    pub MODULE_DEPENDENCIES,
    Deny,
    "cross-module dependency not declared in allowlist"
}

rustc_session::declare_lint! {
    /// Flags modules not listed in the config when exhaustive mode is enabled.
    pub MODULE_DEPENDENCIES_UNLISTED,
    Deny,
    "module not listed in module_dependencies config (exhaustive mode)"
}

rustc_session::declare_lint! {
    /// Flags edges declared in the allowlist that have no corresponding
    /// dependency in code. Stale edges make the config lie.
    pub MODULE_DEPENDENCIES_DEAD_EDGE,
    Warn,
    "allowlist edge has no corresponding dependency in code"
}

// ── Lint pass ────────────────────────────────────────────────────────

#[expect(suggest_builder)]
pub struct ModuleDependencies {
    exhaustive: bool,
    allow: FxHashMap<Symbol, FxHashSet<Symbol>>,
    all_modules: FxHashSet<Symbol>,
    /// Tracks which declared edges were actually observed in code.
    used_edges: FxHashSet<(Symbol, Symbol)>,
    warned_unlisted: FxHashSet<Symbol>,
}

impl ModuleDependencies {
    pub fn new() -> Self {
        let config: ModuleDependenciesConfig =
            dylint_linting::config_or_default("module_dependencies");

        let mut allow: FxHashMap<Symbol, FxHashSet<Symbol>> = FxHashMap::default();
        let mut all_modules = FxHashSet::default();

        for (module, deps) in &config.allow {
            let mod_sym = Symbol::intern(module);
            all_modules.insert(mod_sym);
            let dep_set: FxHashSet<Symbol> = deps.iter().map(|d| Symbol::intern(d)).collect();
            for &dep in &dep_set {
                all_modules.insert(dep);
            }
            allow.insert(mod_sym, dep_set);
        }

        Self {
            exhaustive: config.exhaustive,
            allow,
            all_modules,
            used_edges: FxHashSet::default(),
            warned_unlisted: FxHashSet::default(),
        }
    }

    fn is_configured(&self) -> bool {
        !self.allow.is_empty() || self.exhaustive
    }

    fn check_dependency(&mut self, cx: &LateContext<'_>, def_id: DefId, hir_id: HirId, span: Span) {
        if !self.is_configured() {
            return;
        }

        if !def_id.is_local() {
            return;
        }

        if span.from_expansion() {
            return;
        }

        if cx.sess().is_test_crate() || is_in_test(cx.tcx, hir_id) {
            return;
        }

        let source_mod = cx.tcx.parent_module(hir_id);
        let source_top = top_level_module(cx.tcx, source_mod.to_def_id());
        let target_top = top_level_module(cx.tcx, def_id);

        let (Some(source), Some(target)) = (source_top, target_top) else {
            return;
        };

        // Same module — always OK.
        if source == target {
            return;
        }

        // Exhaustive mode: both modules must be in the config.
        if self.exhaustive {
            if !self.all_modules.contains(&source) && self.warned_unlisted.insert(source) {
                span_lint_and_help(
                    cx,
                    MODULE_DEPENDENCIES_UNLISTED,
                    span,
                    format!("module `{source}` is not listed in module_dependencies config"),
                    None,
                    "add this module to [module_dependencies.allow] in dylint.toml",
                );
                return;
            }
            if !self.all_modules.contains(&target) && self.warned_unlisted.insert(target) {
                span_lint_and_help(
                    cx,
                    MODULE_DEPENDENCIES_UNLISTED,
                    span,
                    format!("module `{target}` is not listed in module_dependencies config"),
                    None,
                    "add this module to [module_dependencies.allow] in dylint.toml",
                );
                return;
            }
        }

        // If the source module isn't in the config at all (non-exhaustive mode),
        // don't enforce — only configured modules are checked.
        let Some(allowed) = self.allow.get(&source) else {
            return;
        };

        // Record for dead-edge detection (only for configured modules).
        self.used_edges.insert((source, target));

        if allowed.contains(&target) {
            return;
        }

        let mut allowed_list: Vec<_> = allowed.iter().map(|s| s.as_str().to_owned()).collect();
        allowed_list.sort();
        let allowed_str = if allowed_list.is_empty() {
            "none".to_owned()
        } else {
            allowed_list.join(", ")
        };

        span_lint_and_help(
            cx,
            MODULE_DEPENDENCIES,
            span,
            format!("`{source}` depends on `{target}`, which is not in its allowlist"),
            None,
            format!(
                "if this dependency is architecturally correct, add \"{target}\" to the \
                 `{source}` allowlist in dylint.toml under [module_dependencies.allow]\n\
                 if not, move the item to a module that `{source}` is allowed to depend on \
                 (currently: {allowed_str})"
            ),
        );
    }
}

rustc_session::impl_lint_pass!(ModuleDependencies => [MODULE_DEPENDENCIES, MODULE_DEPENDENCIES_UNLISTED, MODULE_DEPENDENCIES_DEAD_EDGE]);

impl<'tcx> LateLintPass<'tcx> for ModuleDependencies {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only Path, Struct, and MethodCall carry cross-module references; all other variants are irrelevant"
    )]
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        match &expr.kind {
            // Path expressions: `crate::foo::Bar`, `foo::bar()`, etc.
            ExprKind::Path(qpath) => {
                if let Res::Def(_, def_id) = cx.qpath_res(qpath, expr.hir_id) {
                    self.check_dependency(cx, def_id, expr.hir_id, expr.span);
                }
            }
            // Struct literals: `Foo { field: val }`
            ExprKind::Struct(qpath, _, _) => {
                if let Res::Def(_, def_id) = cx.qpath_res(qpath, expr.hir_id) {
                    self.check_dependency(cx, def_id, expr.hir_id, expr.span);
                }
            }
            // Method calls: `receiver.method()`
            ExprKind::MethodCall(..) => {
                if let Some(def_id) = cx.typeck_results().type_dependent_def_id(expr.hir_id) {
                    self.check_dependency(cx, def_id, expr.hir_id, expr.span);
                }
            }
            _ => {}
        }
    }

    fn check_ty(
        &mut self,
        cx: &LateContext<'tcx>,
        ty: &'tcx rustc_hir::Ty<'tcx, rustc_hir::AmbigArg>,
    ) {
        if let rustc_hir::TyKind::Path(ref qpath) = ty.kind
            && let Res::Def(_, def_id) = cx.qpath_res(qpath, ty.hir_id)
        {
            self.check_dependency(cx, def_id, ty.hir_id, ty.span);
        }
    }

    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        // Catch `use` statements.
        if let ItemKind::Use(path, _) = &item.kind {
            for res in path.res.iter().flatten() {
                if let Res::Def(_, def_id) = res {
                    self.check_dependency(cx, *def_id, item.hir_id(), item.span);
                }
            }
        }
    }

    fn check_crate_post(&mut self, cx: &LateContext<'tcx>) {
        if !self.is_configured() {
            return;
        }

        for (source, deps) in &self.allow {
            for target in deps {
                if !self.used_edges.contains(&(*source, *target)) {
                    span_lint_and_help(
                        cx,
                        MODULE_DEPENDENCIES_DEAD_EDGE,
                        rustc_span::DUMMY_SP,
                        format!(
                            "allowlist declares `{source}` \u{2192} `{target}`, \
                             but no such dependency exists in code"
                        ),
                        None,
                        format!(
                            "remove \"{target}\" from the `{source}` allowlist \
                             in dylint.toml, or this edge is stale"
                        ),
                    );
                }
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Extracts the top-level module name for a local `DefId`.
///
/// Returns `None` for items at the crate root or external crate items.
/// The first `TypeNs` component of the def path is the top-level module.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "only TypeNs represents a named module; all other DefPathData variants are irrelevant"
)]
fn top_level_module(tcx: TyCtxt<'_>, def_id: DefId) -> Option<Symbol> {
    if !def_id.is_local() {
        return None;
    }
    let def_path = tcx.def_path(def_id);
    let first = def_path.data.first()?;
    match first.data {
        DefPathData::TypeNs(sym) => Some(sym),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use dylint_testing::ui;

    const TOML: &str = "\
[module_dependencies]\n\
exhaustive = false\n\
\n\
[module_dependencies.allow]\n\
types = []\n\
errors = [\"types\"]\n\
utils = [\"types\", \"errors\"]\n\
payments = [\"types\", \"errors\", \"utils\"]\n\
server = [\"types\", \"errors\", \"utils\"]\n\
";

    #[test]
    fn ui_module_dependencies() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "module_dependencies")
            .dylint_toml(TOML)
            .run();
    }
}

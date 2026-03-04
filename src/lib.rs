#![feature(rustc_private)]

extern crate rustc_ast;
extern crate rustc_hir;
extern crate rustc_lint;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;

mod config;
mod lints;

use rustc_lint::LintStore;
use rustc_session::Session;

dylint_linting::dylint_library!();

#[expect(clippy::no_mangle_with_rust_abi)]
#[unsafe(no_mangle)]
pub fn register_lints(sess: &Session, lint_store: &mut LintStore) {
    dylint_linting::init_config(sess);
    lint_store.register_lints(&[
        lints::suggest_builder::SUGGEST_BUILDER,
        lints::needless_builder::NEEDLESS_BUILDER,
        lints::large_struct::LARGE_STRUCT,
        lints::bon_builder_collector::BON_BUILDER_COLLECTOR,
    ]);
    lint_store.register_pre_expansion_pass(|| Box::new(lints::bon_builder_collector::BonBuilderCollector));
    lint_store.register_late_pass(|_| Box::new(lints::suggest_builder::SuggestBuilder::new()));
    lint_store.register_late_pass(|_| Box::new(lints::needless_builder::NeedlessBuilder::new()));
    lint_store.register_late_pass(|_| Box::new(lints::large_struct::LargeStruct::new()));
}

#[cfg(test)]
mod tests {
    use dylint_testing::ui;

    #[test]
    fn ui_suggest_builder() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "suggest_builder")
            .dylint_toml("[suggest_builder]\nthreshold = 4\n[needless_builder]\nthreshold = 0\n[large_struct]\nthreshold = 999\n")
            .run();
    }

    #[test]
    fn ui_needless_builder() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "needless_builder")
            .dylint_toml("[suggest_builder]\nthreshold = 999\n[needless_builder]\nthreshold = 2\n[large_struct]\nthreshold = 999\n")
            .run();
    }

    #[test]
    fn ui_large_struct() {
        ui::Test::example(env!("CARGO_PKG_NAME"), "large_struct")
            .dylint_toml("[suggest_builder]\nthreshold = 999\n[needless_builder]\nthreshold = 999\n[large_struct]\nthreshold = 12\n")
            .run();
    }
}

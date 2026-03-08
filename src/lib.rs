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

#[expect(
    clippy::no_mangle_with_rust_abi,
    reason = "dylint requires extern fn signature"
)]
#[expect(
    unsafe_code,
    reason = "dylint requires #[no_mangle] for plugin registration"
)]
#[unsafe(no_mangle)]
pub fn register_lints(sess: &Session, lint_store: &mut LintStore) {
    dylint_linting::init_config(sess);
    lint_store.register_lints(&[
        lints::suggest_builder::SUGGEST_BUILDER,
        lints::needless_builder::NEEDLESS_BUILDER,
        lints::bon_builder_collector::BON_BUILDER_COLLECTOR,
    ]);
    lint_store.register_pre_expansion_pass(|| {
        Box::new(lints::bon_builder_collector::BonBuilderCollector)
    });
    lint_store.register_late_pass(|_| Box::new(lints::suggest_builder::SuggestBuilder::new()));
    lint_store.register_late_pass(|_| Box::new(lints::needless_builder::NeedlessBuilder::new()));
}

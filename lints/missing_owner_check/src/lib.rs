#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_ast;
extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use anchor_lints_utils::mir_analyzer::MirAnalyzer;

use clippy_utils::{diagnostics::span_lint, fn_has_unsatisfiable_preds};
use rustc_hir::{Body as HirBody, FnDecl, def_id::LocalDefId, intravisit::FnKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_span::Span;

mod utils;
use utils::*;

dylint_linting::declare_late_lint! {
    /// ### What it does
    /// Detects when an `UncheckedAccount` or `AccountInfo` has its data accessed
    /// without a statically detectable owner validation.
    ///
    /// ### Why is this bad?
    /// Missing owner validation allows attackers to pass accounts owned by unexpected programs,
    /// leading to reading or modifying data from wrong accounts, security vulnerabilities, and state corruption.
    ///
    /// ### Bad
    /// ```rust
    /// pub metadata: UncheckedAccount<'info>;
    ///
    /// let data = metadata.to_account_info().data.borrow();
    /// ```
    ///
    /// ### Good
    /// ```rust
    /// #[account(owner = mpl_token_metadata::ID)]
    /// pub metadata: UncheckedAccount<'info>;
    /// ```
    pub MISSING_OWNER_CHECK,
    Warn,
    "account data is accessed without a detectable owner check"
}

impl<'tcx> LateLintPass<'tcx> for MissingOwnerCheck {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        _kind: FnKind<'tcx>,
        _: &'tcx FnDecl<'tcx>,
        body: &'tcx HirBody<'tcx>,
        fn_span: Span,
        def_id: LocalDefId,
    ) {
        // Skip macro expansions
        if fn_span.from_expansion() {
            return;
        }

        // Skip functions with unsatisfiable predicates
        if fn_has_unsatisfiable_preds(cx, def_id.to_def_id()) {
            return;
        }

        analyze_missing_owner_check(cx, body, def_id);
    }
}

fn analyze_missing_owner_check<'tcx>(
    cx: &LateContext<'tcx>,
    body: &'tcx HirBody<'tcx>,
    def_id: LocalDefId,
) {
    let mut mir_analyzer = MirAnalyzer::new(cx, body, def_id);

    // Update anchor context info with accounts
    if mir_analyzer.anchor_context_info.is_none() {
        mir_analyzer.update_anchor_context_info_with_context_accounts(body);
    }

    // Analyze functions that take Anchor context
    let Some(anchor_context) = mir_analyzer.anchor_context_info.as_ref() else {
        return;
    };

    // extract accounts that need owner validation
    let accounts_needing_check = extract_accounts_needing_owner_check(cx, anchor_context);

    if accounts_needing_check.is_empty() {
        return;
    }

    // extract accounts with data access
    let accounts_with_data_access =
        extract_accounts_with_data_access(cx, &mir_analyzer, anchor_context);

    for (account_name, account_info) in accounts_needing_check {
        if account_needs_owner_check(&account_info, &accounts_with_data_access) {
            span_lint(
                cx,
                MISSING_OWNER_CHECK,
                account_info.span,
                format!(
                    "account `{}` has its data accessed but no owner validation detected — consider adding `#[account(owner = <program>)]`, using `Account<'info, T>`, or ensure manual validation is performed",
                    account_name
                ),
            );
        }
    }
}

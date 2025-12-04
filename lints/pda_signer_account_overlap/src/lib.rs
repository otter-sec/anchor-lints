#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use anchor_lints_utils::{
    mir_analyzer::MirAnalyzer,
    models::{PdaSigner, UnsafeAccount},
    utils::get_hir_body_from_local_def_id,
};

use clippy_utils::fn_has_unsatisfiable_preds;
use rustc_hir::{
    Body as HirBody, FnDecl,
    def_id::LocalDefId,
    intravisit::FnKind,
};
use rustc_lint::{LateContext, LateLintPass};
use rustc_span::Span;

use std::cell::RefCell;
use std::collections::HashSet;

mod pda_analyzer;
mod utils;

use pda_analyzer::*;
use utils::*;

dylint_linting::declare_late_lint! {
    /// ### What it does
    /// Detects when user-controlled accounts (UncheckedAccount or Option<UncheckedAccount>)
    /// are passed to CPIs that use PDAs as signers, which could lead to PDA initialization
    /// vulnerabilities if the callee expects the account to be uninitialized.
    ///
    /// ### Why is this bad?
    /// If a user-controlled account is passed to a CPI that uses a PDA signer, and the callee
    /// expects the account to be uninitialized and a signer, an attacker could pass the PDA
    /// signer itself as the account. This would cause the PDA to be initialized, losing its
    /// lamports and potentially causing security vulnerabilities.
    ///
    /// ### Example
    /// ```rust
    /// // Bad: User-controlled account passed to CPI with PDA signer
    /// #[account(mut)]
    /// pub second_position_nft_mint: Option<UncheckedAccount<'info>>,
    /// // ... later in CPI call with pool_authority (PDA) as signer
    /// damm_v2::cpi::create_position(CpiContext::new_with_signer(
    ///     program,
    ///     accounts,
    ///     &[&pool_authority_seeds[..]],
    /// ))?;
    /// ```
    pub PDA_SIGNER_ACCOUNT_OVERLAP,
    Warn,
    "user-controlled account passed to CPI with PDA signer — potential PDA initialization vulnerability"
}

// Thread local variable to store analyzed functions to avoid duplicate analysis
thread_local! {
    static ANALYZED_FUNCTIONS: RefCell<HashSet<LocalDefId>> = RefCell::new(HashSet::new());
}

impl<'tcx> LateLintPass<'tcx> for PdaSignerAccountOverlap {
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
        // Skip test files
        if is_test_file(cx, fn_span) {
            return;
        }
        // Skip if already analyzed
        let already_analyzed =
            ANALYZED_FUNCTIONS.with(|analyzed| analyzed.borrow().contains(&def_id));

        if already_analyzed {
            return;
        }

        analyze_pda_signer_account_overlap(cx, body, def_id);
    }
}

/// Analyze a function for CPI calls with unsafe accounts and PDA signers
fn analyze_pda_signer_account_overlap<'tcx>(
    cx: &LateContext<'tcx>,
    body: &HirBody<'tcx>,
    def_id: LocalDefId,
) {
    let mir_analyzer = MirAnalyzer::new(cx, body, def_id);

    // Only analyze functions that take Anchor context
    let Some(anchor_context_info) = mir_analyzer.anchor_context_info.as_ref() else {
        return;
    };

    // Extract unsafe accounts and PDA signers from context
    let (unsafe_accounts, pda_signers) = mir_analyzer.extract_unsafe_accounts_and_pdas();

    // Skip if no unsafe accounts or PDA signers found
    if unsafe_accounts.is_empty() || pda_signers.is_empty() {
        return;
    }

    let analyzer = PdaSignerAnalyzer::new(unsafe_accounts.clone(), pda_signers.clone(), def_id);
    analyzer.analyze_basic_blocks(&mir_analyzer, anchor_context_info);
}

/// Analyze a nested function for CPI calls with unsafe accounts and PDA signers
fn analyze_nested_function_for_cpi<'tcx>(
    cx: &LateContext<'tcx>,
    body: &HirBody<'tcx>,
    def_id: LocalDefId,
    unsafe_accounts: &[UnsafeAccount],
    pda_signers: &[PdaSigner],
    anchor_context_info: &anchor_lints_utils::mir_analyzer::AnchorContextInfo<'tcx>,
) {
    // Mark as analyzed to avoid infinite recursion
    ANALYZED_FUNCTIONS.with(|analyzed| {
        analyzed.borrow_mut().insert(def_id);
    });

    let mir_analyzer = MirAnalyzer::new(cx, body, def_id);

    let analyzer = PdaSignerAnalyzer::new(unsafe_accounts.to_vec(), pda_signers.to_vec(), def_id);
    analyzer.analyze_basic_blocks(&mir_analyzer, anchor_context_info);
}

/// Recursively analyze a nested function for CPI calls with unsafe accounts and PDA signers
fn analyze_nested_function_if_available<'tcx>(
    cx: &LateContext<'tcx>,
    fn_def_id: LocalDefId,
    unsafe_accounts: &[UnsafeAccount],
    pda_signers: &[PdaSigner],
    anchor_context_info: &anchor_lints_utils::mir_analyzer::AnchorContextInfo<'tcx>,
) {
    if let Some(nested_body) = get_hir_body_from_local_def_id(cx, fn_def_id) {
        let nested_body = cx.tcx.hir_body(nested_body);
        analyze_nested_function_for_cpi(
            cx,
            nested_body,
            fn_def_id,
            unsafe_accounts,
            pda_signers,
            anchor_context_info,
        );
    }
}

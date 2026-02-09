#![feature(rustc_private)]
#![warn(unused_extern_crates)]

extern crate rustc_ast;
extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use anchor_lints_utils::{
    diag_items::is_anchor_system_account_type,
    mir_analyzer::{AnchorContextInfo, MirAnalyzer},
    utils::{pda_detection::is_pda_account, should_skip_function},
};
use clippy_utils::diagnostics::span_lint;

use rustc_hir::{Body as HirBody, FnDecl, def_id::LocalDefId, intravisit::FnKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty::TyKind;
use rustc_span::Span;

use std::collections::HashSet;

mod checks;
use checks::*;

dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Detects when a seed account used in PDA derivation is overconstrained as `SystemAccount`
    /// in non-initialization instructions.
    ///
    /// ### Why is this bad?
    /// If a seed account's ownership changes after pool creation (e.g., becomes a token account
    /// or mint), future instructions will fail forever because `SystemAccount` enforces
    /// `owner == system_program`. This can permanently lock funds in the protocol.
    ///
    /// ### Example
    /// ```rust
    /// #[derive(Accounts)]
    /// pub struct Withdraw<'info> {
    ///     #[account(
    ///         seeds = [b"pool", creator.key().as_ref()],
    ///         bump
    ///     )]
    ///     pub pool: Account<'info, Pool>,
    ///
    ///     // Overconstrained seed-only account
    ///     pub creator: SystemAccount<'info>,
    /// }
    /// ```
    ///
    /// ### Good
    /// ```rust
    /// pub creator: UncheckedAccount<'info>,  // or AccountInfo<'info>
    /// ```
    pub OVERCONSTRAINED_SEED_ACCOUNT,
    Warn,
    "seed-only account is overconstrained as SystemAccount",
    OverconstrainedSeedAccount
}

#[derive(Default)]
pub struct OverconstrainedSeedAccount;

impl<'tcx> LateLintPass<'tcx> for OverconstrainedSeedAccount {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        _kind: FnKind<'tcx>,
        _: &FnDecl<'tcx>,
        body: &HirBody<'tcx>,
        main_fn_span: Span,
        def_id: LocalDefId,
    ) {
        // Skip macro expansions, unsatisfiable predicates, and test files
        if should_skip_function(cx, main_fn_span, def_id) {
            return;
        }

        let mut mir_analyzer = MirAnalyzer::new(cx, body, def_id);

        // Update anchor context info with accounts
        anchor_lints_utils::utils::ensure_anchor_context_initialized(&mut mir_analyzer, body);

        // Analyze functions that take Anchor context
        let Some(anchor_context_info) = mir_analyzer.anchor_context_info.as_ref() else {
            return;
        };

        analyze_overconstrained_seed_accounts(cx, &mir_analyzer, anchor_context_info);
    }
}

fn analyze_overconstrained_seed_accounts<'cx, 'tcx>(
    cx: &'cx LateContext<'tcx>,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    anchor_context_info: &AnchorContextInfo<'tcx>,
) {
    // Skip if this is an init instruction
    if is_init_instruction(cx, anchor_context_info) {
        return;
    }

    let accounts_struct_ty = &anchor_context_info.anchor_context_account_type;

    if let TyKind::Adt(adt_def, generics) = accounts_struct_ty.kind() {
        if !adt_def.is_struct() && !adt_def.is_union() {
            return;
        }

        let variant = adt_def.non_enum_variant();

        // Find all PDAs and their seed accounts
        let mut pda_seed_accounts = HashSet::new();

        for field in &variant.fields {
            if is_pda_account(cx, field).is_some() {
                // Extract account names from seeds
                let seed_accounts = extract_seed_accounts_from_pda(cx, field);
                for seed_account in seed_accounts {
                    pda_seed_accounts.insert(seed_account);
                }
            }
        }

        // Check each account field
        for field in &variant.fields {
            let account_name = field.ident(cx.tcx).to_string();
            let account_span = cx.tcx.def_span(field.did);
            let account_ty = field.ty(cx.tcx, generics);

            // Check if this account is a SystemAccount
            if !is_anchor_system_account_type(cx.tcx, account_ty) {
                continue;
            }

            // Check if this account is used as a seed
            if !pda_seed_accounts.contains(&account_name) {
                continue;
            }

            // Check if account is only used for seeds
            if is_account_required(cx, field, &account_name, anchor_context_info, mir_analyzer) {
                continue;
            }

            span_lint(
                cx,
                OVERCONSTRAINED_SEED_ACCOUNT,
                account_span,
                format!(
                    "seed-only account `{}` is overconstrained as `SystemAccount`. If this account's ownership changes, PDA validation will fail and funds may be permanently locked. Consider using `UncheckedAccount` for PDA seeds in non-init instructions.",
                    account_name
                ),
            );
        }
    }
}

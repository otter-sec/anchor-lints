#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use anchor_lints_utils::mir_analyzer::{AnchorContextInfo, MirAnalyzer};

use clippy_utils::{diagnostics::span_lint_and_note, fn_has_unsatisfiable_preds};

use rustc_hir::{Body as HirBody, FnDecl, def_id::LocalDefId, intravisit::FnKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::{
    mir::{Operand, StatementKind, TerminatorKind},
    ty::{self as rustc_ty},
};
use rustc_span::Span;

use std::collections::HashMap;

mod utils;
use utils::*;

dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Detects when accounts with direct lamport mutations (via `lamports.borrow_mut()`)
    /// are not included in subsequent CPI calls, which can cause runtime balance check errors.
    ///
    /// ### Why is this bad?
    /// When a program directly mutates lamports and then performs a CPI, the Solana runtime
    /// performs a balance check. If a lamport-mutated account is not included in the CPI accounts
    /// (either as a direct account or in `with_remaining_accounts`), the runtime will throw an error,
    /// effectively creating a DoS in the protocol.
    ///
    /// ### Example
    /// ```rust
    /// Bad: fee_collector's lamports were mutated but not included in CPI
    /// ctx.accounts.vault.lamports.borrow_mut() -= WITHDRAW_FEE;
    /// ctx.accounts.fee_collector.lamports.borrow_mut() += WITHDRAW_FEE;
    ///
    /// token::transfer(
    ///     CpiContext::new_with_signer(
    ///         ctx.accounts.token_program.to_account_info(),
    ///         Transfer {
    ///             from: ctx.accounts.vault_token.to_account_info(),
    ///             to: ctx.accounts.user_token.to_account_info(),
    ///             authority: ctx.accounts.vault.to_account_info(),
    ///         },
    ///         signer_seeds,
    ///     ),
    ///     amount,
    /// )?;
    /// ```
    ///
    /// ### Good
    /// ```rust
    /// Good: fee_collector is included in remaining_accounts
    /// ctx.accounts.vault.lamports.borrow_mut() -= WITHDRAW_FEE;
    /// ctx.accounts.fee_collector.lamports.borrow_mut() += WITHDRAW_FEE;
    ///
    /// token::transfer(
    ///     CpiContext::new_with_signer(
    ///         ctx.accounts.token_program.to_account_info(),
    ///         Transfer {
    ///             from: ctx.accounts.vault_token.to_account_info(),
    ///             to: ctx.accounts.user_token.to_account_info(),
    ///             authority: ctx.accounts.vault.to_account_info(),
    ///         },
    ///         signer_seeds,
    ///     )
    ///     .with_remaining_accounts(vec![
    ///         ctx.accounts.fee_collector.to_account_info(),
    ///     ]),
    ///     amount,
    /// )?;
    /// ```
    pub DIRECT_LAMPORT_CPI_DOS,
    Warn,
    "lamport-mutated account not included in subsequent CPI",
    DirectLamportCpiDos
}

#[derive(Default)]
pub struct DirectLamportCpiDos;

impl<'tcx> LateLintPass<'tcx> for DirectLamportCpiDos {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        _kind: FnKind<'tcx>,
        _: &FnDecl<'tcx>,
        body: &HirBody<'tcx>,
        main_fn_span: Span,
        def_id: LocalDefId,
    ) {
        // Skip macro expansions
        if main_fn_span.from_expansion() {
            return;
        }

        // skip functions with unsatisfiable predicates
        if fn_has_unsatisfiable_preds(cx, def_id.to_def_id()) {
            return;
        }

        let mut mir_analyzer = MirAnalyzer::new(cx, body, def_id);

        // Update anchor context info with accounts
        if mir_analyzer.anchor_context_info.is_none() {
            mir_analyzer.update_anchor_context_info_with_context_accounts(body);
        }

        // Analyze functions that take Anchor context
        let Some(anchor_context_info) = mir_analyzer.anchor_context_info.as_ref() else {
            return;
        };

        analyze_direct_lamport_cpi_dos(cx, &mir_analyzer, anchor_context_info);
    }
}

fn analyze_direct_lamport_cpi_dos<'cx, 'tcx>(
    cx: &'cx LateContext<'tcx>,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    anchor_context_info: &AnchorContextInfo<'tcx>,
) {
    let mir = mir_analyzer.mir;

    // Track accounts with direct lamport mutations
    let mut lamport_mutated_accounts: HashMap<String, LamportMutation> = HashMap::new();

    // Track CPI calls and their associated accounts
    let mut cpi_calls: Vec<CpiCallInfo> = Vec::new();

    // Detect lamport mutations and CPI calls
    for (bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        // Check statements for lamport mutations
        for stmt in &bbdata.statements {
            if let StatementKind::Assign(box (place, rvalue)) = &stmt.kind
                && let Some(account_name) =
                    detect_lamport_mutation(cx, mir_analyzer, place, rvalue, anchor_context_info)
            {
                lamport_mutated_accounts.insert(
                    account_name.clone(),
                    LamportMutation {
                        span: stmt.source_info.span,
                        block: bb,
                    },
                );
            }
        }

        // Check for CPI calls
        if let TerminatorKind::Call {
            func: Operand::Constant(func),
            args,
            fn_span,
            ..
        } = &bbdata.terminator().kind
            && let rustc_ty::FnDef(fn_def_id, _) = func.ty().kind()
        {
            // Check if it's a CPI call function and not a `with_remaining_accounts` method
            if mir_analyzer.takes_cpi_context(&args[..])
                && !is_remaining_accounts_method(cx, *fn_def_id)
            {
                // Extract accounts from CPI context
                let cpi_accounts = extract_cpi_accounts(mir_analyzer, &args[..], bb);
                cpi_calls.push(CpiCallInfo {
                    block: bb,
                    span: *fn_span,
                    accounts: cpi_accounts,
                });
            }
        }
    }

    // Check if lamport-mutated accounts are included in subsequent CPIs
    for (account_name, lamport_mutation) in &lamport_mutated_accounts {
        for cpi_call in &cpi_calls {
            // Check if CPI is reachable from lamport mutation
            if is_reachable(mir_analyzer.mir, lamport_mutation.block, cpi_call.block) {
                // Check if the account is included in the CPI
                if !cpi_call.accounts.contains(account_name) {
                    span_lint_and_note(
                        cx,
                        DIRECT_LAMPORT_CPI_DOS,
                        cpi_call.span,
                        format!(
                            "account `{}` had its lamports directly mutated but is not included in this CPI call",
                            account_name
                        ),
                        Some(lamport_mutation.span),
                        "lamport mutation is here",
                    );
                }
            }
        }
    }
}

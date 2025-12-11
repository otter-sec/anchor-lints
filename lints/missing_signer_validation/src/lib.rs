#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_ast;
extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use anchor_lints_utils::{cpi_types::detect_cpi_kind, mir_analyzer::MirAnalyzer};

use clippy_utils::{diagnostics::span_lint, fn_has_unsatisfiable_preds};
use rustc_hir::{Body as HirBody, FnDecl, def_id::LocalDefId, intravisit::FnKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::{
    mir::{Operand, TerminatorKind},
    ty as rustc_ty,
};
use rustc_span::Span;

use std::collections::HashMap;

mod cpi_rules;
mod utils;
use cpi_rules::*;
use utils::*;

dylint_linting::declare_late_lint! {
    /// ### What it does
    /// Warns when a CPI requires a signer (e.g., authority/owner/current_authority)
    /// but the passed account is **not**:
    /// - declared as a signer (`Signer<'info>` or `#[account(signer)]`), and
    /// - **not** signed via PDA seeds (`CpiContext::new_with_signer`).
    ///
    /// ### Why it's important
    /// Missing signer validation allows attackers to perform unauthorized
    /// token transfers, minting, burning, authority changes, or system transfers.
    ///
    /// ### Bad
    /// ```rust
    /// anchor_spl::token::burn(
    ///     CpiContext::new(
    ///         ctx.accounts.token_program.to_account_info(),
    ///         Burn {
    ///             mint: ctx.accounts.mint.to_account_info(),
    ///             from: ctx.accounts.user_ata.to_account_info(),
    ///             authority: ctx.accounts.pool_authority.to_account_info(), // ❌ not a signer
    ///         },
    ///     ),
    ///     100,
    /// )?;
    /// ```
    ///
    /// ### Good
    /// ```rust
    /// #[account(signer)]        // or: pub authority: Signer<'info>
    /// pub authority: AccountInfo<'info>;
    /// ```
    ///
    /// ### Good (PDA Signer)
    /// ```rust
    /// CpiContext::new_with_signer(..., &[&seeds]); // ✅ PDA validated as signer
    /// ```
    ///
    pub MISSING_SIGNER_VALIDATION,
    Warn,
    "CPI signer account not validated as signer or PDA signer"
}

impl<'tcx> LateLintPass<'tcx> for MissingSignerValidation {
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

        analyze_missing_signer_validation(cx, body, def_id);
    }
}

fn analyze_missing_signer_validation<'tcx>(
    cx: &LateContext<'tcx>,
    body: &'tcx HirBody<'tcx>,
    def_id: LocalDefId,
) {
    let mir = cx.tcx.optimized_mir(def_id.to_def_id());

    let mut mir_analyzer = MirAnalyzer::new(cx, body, def_id);

    // Update anchor context info with accounts
    if mir_analyzer.anchor_context_info.is_none() {
        mir_analyzer.update_anchor_context_info_with_context_accounts(body);
    }

    // Analyze functions that take Anchor context
    let Some(anchor_context) = mir_analyzer.anchor_context_info.as_ref() else {
        return;
    };

    // Extract accounts with signer attribute/type
    let accounts_with_signer = extract_accounts_with_signer_attribute(cx, anchor_context);

    // Track accounts used as signers in CPIs
    let mut accounts_used_as_signer: HashMap<String, Span> = HashMap::new();

    // Analyze MIR for CPI calls and identify signer accounts
    for (bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        if let TerminatorKind::Call {
            func: Operand::Constant(func_const),
            args,
            ..
        } = &bbdata.terminator().kind
            && let rustc_ty::FnDef(fn_def_id, _) = func_const.ty().kind()
        {
            let Some(cpi_kind) = detect_cpi_kind(cx, *fn_def_id) else {
                continue;
            };

            let Some(cpi_meta) = get_cpi_rule(cpi_kind) else {
                continue;
            };
            match cpi_meta.signer_source {
                SignerSource::ContextSigner => {
                    // signer comes from CpiContext::new(...)
                    if let Some(cpi_ctx_local) = extract_arg_local(args, 0) {
                        accounts_used_as_signer.extend(extract_cpi_accounts_from_context(
                            &mir_analyzer,
                            bb,
                            cpi_ctx_local,
                            cpi_meta,
                        ));
                    }
                }

                SignerSource::ArgIndex(idx) => {
                    insert_authority_from_arg(
                        &mir_analyzer,
                        args,
                        idx,
                        &mut accounts_used_as_signer,
                    );
                }
            }
        }
    }

    // Check each account used as signer for validation
    for (account_name, cpi_span) in accounts_used_as_signer {
        let has_signer_attr = accounts_with_signer.contains(&account_name);
        if !has_signer_attr {
            span_lint(
                cx,
                MISSING_SIGNER_VALIDATION,
                cpi_span,
                format!(
                    "account `{}` is used as a signer but lacks signer validation — add `#[account(signer)]`",
                    account_name
                ),
            );
        }
    }
}

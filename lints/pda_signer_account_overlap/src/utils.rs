use std::collections::HashSet;

use anchor_lints_utils::{
    diag_items::{DiagnoticItem, is_anchor_cpi_context},
    mir_analyzer::MirAnalyzer,
};
use clippy_utils::source::HasSession;
use rustc_hir::def_id::DefId;
use rustc_lint::LateContext;
use rustc_middle::mir::Operand;
use rustc_span::{Span, source_map::Spanned};

/// Check if an account has a constraint preventing it from being the same as another account
pub fn has_constraint_preventing_overlap(
    constraints: &[String],
    account_name: &str,
    pda_account_name: &str,
) -> bool {
    // Check if the account has a constraint preventing it from being the same as another account
    for constraint in constraints {
        if constraint.contains(account_name)
            && constraint.contains(pda_account_name)
            && constraint.contains("!=")
        {
            return true;
        }
        if constraint.contains(pda_account_name)
            && constraint.contains(account_name)
            && constraint.contains("!=")
        {
            return true;
        }
    }
    false
}

// check if the CPI call uses a PDA signer
pub fn check_cpi_uses_pda_signer<'tcx>(
    cx: &LateContext<'tcx>,
    fn_def_id: DefId,
    args: &[Spanned<Operand<'tcx>>],
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
) -> bool {
    if DiagnoticItem::AnchorCpiInvokeSigned.defid_is_item(cx.tcx, fn_def_id)
        || DiagnoticItem::AnchorCpiInvokeSignedUnchecked.defid_is_item(cx.tcx, fn_def_id)
    {
        return true;
    }

    if mir_analyzer.takes_cpi_context(args) {
        return args.len() >= 3;
    }

    let fn_sig = cx.tcx.fn_sig(fn_def_id).skip_binder();
    let return_ty = fn_sig.skip_binder().output();

    if is_anchor_cpi_context(cx.tcx, return_ty) && args.len() >= 2 {
        return true;
    }

    false
}

pub fn extract_accounts_passed_to_cpi<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
    anchor_context_info: Option<&anchor_lints_utils::mir_analyzer::AnchorContextInfo<'tcx>>,
) -> HashSet<String> {
    let mut accounts_passed = HashSet::new();

    if let Some(cpi_accounts_struct) = args.get(1)
        && let Operand::Copy(place) | Operand::Move(place) = &cpi_accounts_struct.node
        && let Some(accounts_local) = place.as_local()
        && let Some(accounts) =
            mir_analyzer.find_cpi_accounts_struct(&accounts_local, &mut HashSet::new())
    {
        for account_local in accounts {
            if let Some(account_info) =
                mir_analyzer.is_from_cpi_context(account_local, anchor_context_info)
            {
                accounts_passed.insert(account_info.account_name);
            }
        }
    }

    accounts_passed
}

use std::collections::HashSet;

use anchor_lints_utils::{
    diag_items::{DiagnoticItem, is_anchor_cpi_context},
    mir_analyzer::MirAnalyzer,
};
use clippy_utils::source::HasSession;
use rustc_hir::def_id::DefId;
use rustc_lint::LateContext;
use rustc_middle::{
    mir::{HasLocalDecls, Operand},
    ty::{Ty, TyKind},
};
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

/// Check if test file
pub fn is_test_file(cx: &LateContext, span: Span) -> bool {
    use rustc_span::{FileName, FileNameDisplayPreference};
    let file_name = cx.sess().source_map().span_to_filename(span);
    match file_name {
        FileName::Real(ref path) => {
            let path_str = path.to_string_lossy(FileNameDisplayPreference::Local);
            path_str.contains("test") || path_str.contains("tests")
        }
        _ => false,
    }
}

pub fn compare_adt_def_ids(ty1: Ty, ty2: Ty) -> bool {
    if let (TyKind::Adt(adt1, _), TyKind::Adt(adt2, _)) = (ty1.kind(), ty2.kind()) {
        adt1.did() == adt2.did()
    } else {
        false
    }
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

// check if the CPI call is new_with_signer
pub fn check_cpi_call_is_new_with_signer<'tcx>(
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
    args: &[Spanned<Operand<'tcx>>],
    fn_def_id: DefId,
) -> bool {
    if let Some(fn_name) = mir_analyzer.cx.tcx.opt_item_name(fn_def_id) {
        let fn_name_str = fn_name.to_string();
        return fn_name_str == "new_with_signer" && args.len() >= 3;
    }
    false
}

/// Checks if the first argument of a function call is an implementation method
pub fn is_implementation_method<'tcx>(
    mir: &rustc_middle::mir::Body<'tcx>,
    args: &[Spanned<Operand<'tcx>>],
    anchor_context_info: &anchor_lints_utils::mir_analyzer::AnchorContextInfo<'tcx>,
) -> bool {
    args.first()
        .and_then(|arg| {
            if let Operand::Copy(place) | Operand::Move(place) = &arg.node {
                place.as_local().and_then(|local| {
                    mir.local_decls().get(local).map(|decl| {
                        let ty = decl.ty.peel_refs();
                        // Check if it's a reference type (could be &self)
                        if let TyKind::Ref(_, inner_ty, _) = ty.kind() {
                            let inner_ty = inner_ty.peel_refs();
                            compare_adt_def_ids(
                                inner_ty,
                                anchor_context_info.anchor_context_account_type,
                            )
                        } else {
                            compare_adt_def_ids(ty, anchor_context_info.anchor_context_account_type)
                        }
                    })
                })
            } else {
                None
            }
        })
        .unwrap_or(false)
}

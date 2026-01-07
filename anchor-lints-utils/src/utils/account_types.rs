use rustc_lint::LateContext;
use rustc_middle::ty::{Ty, TyKind};

/// Check if a type is Account<'info, T>
pub fn is_anchor_account_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, _) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        def_path.contains("anchor_lang::prelude::Account")
            && !def_path.contains("AccountInfo")
            && !def_path.contains("UncheckedAccount")
    } else {
        false
    }
}

/// Check if a type is SystemAccount
pub fn is_system_account_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, _) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        return def_path == "anchor_lang::prelude::SystemAccount"
            || def_path.contains("SystemAccount");
    }
    false
}

/// Check if a type is AccountLoader<'info, T>
pub fn is_account_loader_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, _) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        return def_path.starts_with("anchor_lang::prelude::AccountLoader");
    }
    false
}

/// Check if a type is AccountInfo
pub fn is_account_info_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, _) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        return def_path == "solana_program::account_info::AccountInfo"
            || def_path.ends_with("::AccountInfo");
    }
    false
}

/// Extract the inner type from `Account<'info, T>` or `AccountLoader<'info, T>`
pub fn extract_inner_account_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, substs) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        if (def_path.contains("anchor_lang::prelude::Account")
            || def_path.ends_with("anchor_lang::accounts::account::Account"))
            && !substs.is_empty()
            && let Some(inner_ty) = substs.types().next()
        {
            return Some(inner_ty);
        }
    }
    None
}

/// Check if a type is SystemProgram
pub fn is_system_program_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, _) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        return def_path.contains("SystemProgram") || def_path.contains("system_program");
    }
    false
}

/// Check if a type is a standard SPL token account type (TokenAccount or Mint)
pub fn is_spl_token_account_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, _) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());

        // Check for standard SPL token account types
        if def_path.contains("anchor_spl::token::TokenAccount")
            || def_path.contains("anchor_spl::token_interface::TokenAccount")
            || def_path.contains("spl_token::state::Account")
            || def_path.contains("anchor_spl::token::Mint")
            || def_path.contains("anchor_spl::token_interface::Mint")
            || def_path.contains("spl_token::state::Mint")
        {
            return true;
        }
    }
    false
}

/// Check if a type is Signer
pub fn is_signer_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Adt(adt_def, _) => {
            let path = cx.tcx.def_path_str(adt_def.did());
            path == "anchor_lang::prelude::Signer"
        }
        _ => false,
    }
}

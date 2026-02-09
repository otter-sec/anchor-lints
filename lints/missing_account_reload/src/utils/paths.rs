use anchor_lints_utils::{
    diag_items::{
        is_account_info_type, is_anchor_account_loader_type, is_anchor_account_type,
        is_anchor_interface_account_type, is_anchor_signer_type, is_anchor_system_account_type,
        is_anchor_unchecked_account_type, is_box_type,
    },
    mir_analyzer::AnchorContextInfo,
};
use rustc_lint::LateContext;
use rustc_middle::{mir::BasicBlock, ty::Ty};

use std::collections::HashMap;

use crate::models::*;

pub fn filter_account_accesses<'tcx>(
    cx: &LateContext<'tcx>,
    account_accesses: HashMap<String, Vec<AccountAccess>>,
    anchor_context_info: &AnchorContextInfo<'tcx>,
    cpi_accounts: &HashMap<String, BasicBlock>,
) -> HashMap<String, Vec<AccountAccess>> {
    let mut filtered_accesses = HashMap::new();
    for (name, accesses) in account_accesses {
        let normalized_name = normalize_account_name(&name);

        let (account_ty, contains_data) = if let Some(account_ty) = anchor_context_info
            .anchor_context_arg_accounts_type
            .get(normalized_name)
        {
            (
                Some(*account_ty),
                contains_deserialized_data(cx, *account_ty),
            )
        } else {
            (None, false)
        };

        let in_cpi = cpi_accounts.contains_key(&name) || cpi_accounts.contains_key(normalized_name);
        let should_flag = account_ty.map(|_| contains_data).unwrap_or(in_cpi);

        if should_flag {
            filtered_accesses.insert(name, accesses);
        }
    }
    filtered_accesses
}

// Checks if an account type contains deserialized data that needs reloading after CPI
pub fn contains_deserialized_data<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();
    if let rustc_middle::ty::TyKind::Adt(_adt_def, substs) = ty.kind() {
        if is_anchor_account_type(cx.tcx, ty)
            && !is_account_info_type(cx.tcx, ty)
            && !is_anchor_unchecked_account_type(cx.tcx, ty)
        {
            return true;
        }
        if is_anchor_account_loader_type(cx.tcx, ty) {
            return false;
        }
        if is_anchor_interface_account_type(cx.tcx, ty) || is_anchor_system_account_type(cx.tcx, ty)
        {
            return true;
        }

        // Signer, UncheckedAccount, and AccountInfo don't contain deserialized data
        if is_anchor_signer_type(cx.tcx, ty)
            || is_anchor_unchecked_account_type(cx.tcx, ty)
            || is_account_info_type(cx.tcx, ty)
            || is_account_info_type(cx.tcx, ty)
        {
            return false;
        }

        // Check for Box<T> wrapper
        if is_box_type(cx.tcx, ty) && !substs.is_empty() {
            let inner_ty = substs.type_at(0);
            return contains_deserialized_data(cx, inner_ty);
        }
    }
    false
}

pub fn normalize_account_name(name: &str) -> &str {
    let stripped = if let Some(idx) = name.find(".accounts.") {
        let start = idx + ".accounts.".len();
        &name[start..]
    } else {
        name
    };
    stripped.split('.').next().unwrap_or(stripped)
}

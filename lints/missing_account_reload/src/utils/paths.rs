use anchor_lints_utils::mir_analyzer::AnchorContextInfo;
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
    if let rustc_middle::ty::TyKind::Adt(adt_def, substs) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());

        if def_path.contains("anchor_lang::prelude::Account")
            && !def_path.contains("AccountInfo")
            && !def_path.contains("UncheckedAccount")
        {
            return true;
        }
        if def_path.contains("anchor_lang::prelude::AccountLoader") {
            return false;
        }
        if def_path.contains("anchor_lang::prelude::InterfaceAccount")
            || def_path == "anchor_lang::prelude::SystemAccount"
        {
            return true;
        }

        // Signer, UncheckedAccount, and AccountInfo don't contain deserialized data
        if def_path.contains("anchor_lang::prelude::Signer")
            || def_path.contains("anchor_lang::prelude::UncheckedAccount")
            || def_path.contains("anchor_lang::prelude::AccountInfo")
            || def_path == "solana_program::account_info::AccountInfo"
        {
            return false;
        }

        // Check for Box<T> wrapper
        if def_path.contains("alloc::boxed::Box") && !substs.is_empty() {
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

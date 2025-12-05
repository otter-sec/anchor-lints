use rustc_hir::Body as HirBody;
use rustc_lint::LateContext;
use rustc_middle::{
    mir::Body as MirBody,
    ty::{self as rustc_ty, TyKind},
};

use std::collections::HashMap;

use super::param_extraction::{extract_param_data, is_single_anchor_account_type};
use crate::mir_analyzer::AnchorContextInfo;

/// Extract account fields from an Adt type
pub(crate) fn extract_account_fields_from_adt<'tcx>(
    cx: &LateContext<'tcx>,
    adt_def: &rustc_ty::AdtDef<'tcx>,
    generics: &rustc_ty::GenericArgsRef<'tcx>,
) -> HashMap<String, rustc_ty::Ty<'tcx>> {
    let variant = adt_def.non_enum_variant();
    let mut accounts = HashMap::new();
    for field in &variant.fields {
        let account_name = field.ident(cx.tcx).to_string();
        let account_ty = field.ty(cx.tcx, generics);
        accounts.insert(account_name, account_ty);
    }
    accounts
}

/// Check if a type is an Anchor Context type
pub(crate) fn is_anchor_context_type(struct_name: &str) -> bool {
    struct_name.ends_with("anchor_lang::context::Context")
        || struct_name.ends_with("anchor_lang::prelude::Context")
}

/// Extract accounts field from a Context type
pub(crate) fn extract_accounts_from_context<'tcx>(
    cx: &LateContext<'tcx>,
    adt_def: &rustc_ty::AdtDef<'tcx>,
    generics: &rustc_ty::GenericArgsRef<'tcx>,
) -> Option<(rustc_ty::Ty<'tcx>, HashMap<String, rustc_ty::Ty<'tcx>>)> {
    let variant = adt_def.non_enum_variant();
    for field in &variant.fields {
        let field_name = field.ident(cx.tcx).to_string();
        if field_name == "accounts" {
            let accounts_struct_ty = field.ty(cx.tcx, generics).peel_refs();
            if let TyKind::Adt(accounts_adt_def, accounts_generics) = accounts_struct_ty.kind() {
                let accounts =
                    extract_account_fields_from_adt(cx, accounts_adt_def, accounts_generics);
                return Some((accounts_struct_ty, accounts));
            }
        }
    }
    None
}

/// Get anchor context accounts from function body
pub fn get_anchor_context_accounts<'tcx>(
    cx: &LateContext<'tcx>,
    mir: &MirBody<'tcx>,
    body: &HirBody<'tcx>,
) -> Option<AnchorContextInfo<'tcx>> {
    if body.params.is_empty() {
        return None;
    }

    for (param_index, param) in body.params.iter().enumerate() {
        let Some(param_data) = extract_param_data(cx, mir, param_index, param) else {
            continue;
        };

        if let (Some((adt_def, generics)), Some(struct_name)) =
            (param_data.adt_def, param_data.struct_name)
            && is_anchor_context_type(&struct_name)
            && let Some((accounts_struct_ty, cpi_ctx_accounts)) =
                extract_accounts_from_context(cx, adt_def, generics)
        {
            return Some(AnchorContextInfo {
                anchor_context_name: param_data.param_name,
                anchor_context_account_type: accounts_struct_ty,
                anchor_context_arg_local: param_data.param_local,
                anchor_context_type: param_data.param_ty,
                anchor_context_arg_accounts_type: cpi_ctx_accounts,
            });
        }
    }
    None
}

/// Get context accounts from function body
pub fn get_context_accounts<'tcx>(
    cx: &LateContext<'tcx>,
    mir: &MirBody<'tcx>,
    body: &HirBody<'tcx>,
) -> Option<AnchorContextInfo<'tcx>> {
    if body.params.is_empty() {
        return None;
    }

    for (param_index, param) in body.params.iter().enumerate() {
        let Some(param_data) = extract_param_data(cx, mir, param_index, param) else {
            continue;
        };

        if let (Some((adt_def, generics)), Some(struct_name)) =
            (param_data.adt_def, param_data.struct_name)
        {
            // Skip single Anchor account types - we only want accounts structs
            if is_single_anchor_account_type(&struct_name) {
                continue;
            }

            // Skip if it's a Context type (use get_anchor_context_accounts for those)
            if is_anchor_context_type(&struct_name) {
                continue;
            }

            let cpi_ctx_accounts = extract_account_fields_from_adt(cx, adt_def, generics);

            // Only return if we found account fields (indicating it's an accounts struct)
            if !cpi_ctx_accounts.is_empty() {
                return Some(AnchorContextInfo {
                    anchor_context_name: param_data.param_name,
                    anchor_context_account_type: param_data.param_ty,
                    anchor_context_arg_local: param_data.param_local,
                    anchor_context_type: param_data.param_ty,
                    anchor_context_arg_accounts_type: cpi_ctx_accounts,
                });
            }
        }
    }
    None
}

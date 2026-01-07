use anchor_lints_utils::{
    mir_analyzer::AnchorContextInfo,
    utils::{
        account_types::{
            extract_inner_account_type, is_account_loader_type, is_spl_token_account_type,
        },
        has_account_constraint,
    },
};
use rustc_lint::LateContext;
use rustc_middle::ty::{Ty, TyKind};

use crate::utils::types::InitAccountInfo;
use std::collections::HashMap;

/// Extract all accounts marked with `#[account(init, ...)]` from an Anchor context.
pub fn extract_init_accounts_and_inner_types<'tcx>(
    cx: &LateContext<'tcx>,
    anchor_ctx: &AnchorContextInfo<'tcx>,
) -> HashMap<String, InitAccountInfo<'tcx>> {
    let mut res = HashMap::new();
    let accounts_struct_ty = &anchor_ctx.anchor_context_account_type;

    if let TyKind::Adt(adt_def, generics) = accounts_struct_ty.kind() {
        if !adt_def.is_struct() && !adt_def.is_union() {
            return res;
        }
        let variant = adt_def.non_enum_variant();
        for field in &variant.fields {
            let account_name = field.ident(cx.tcx).to_string();
            let span = cx.tcx.def_span(field.did);
            let account_ty = field.ty(cx.tcx, generics);
            let is_account_loader = is_account_loader_type(cx, account_ty);
            if !has_account_constraint(cx, field, "init") {
                continue;
            }
            if let Some(inner_ty) = extract_inner_account_type(cx, account_ty) {
                // Skip standard SPL token account types - they're initialized by Anchor automatically
                if is_spl_token_account_type(cx, inner_ty) {
                    continue;
                }
                res.insert(
                    account_name,
                    InitAccountInfo {
                        inner_ty,
                        span,
                        is_account_loader,
                    },
                );
            }
        }
    }

    res
}

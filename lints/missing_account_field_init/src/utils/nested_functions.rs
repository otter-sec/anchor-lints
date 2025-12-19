use anchor_lints_utils::{mir_analyzer::MirAnalyzer, utils::get_hir_body_from_local_def_id};
use rustc_hir::def_id::DefId;
use rustc_lint::LateContext;
use rustc_middle::mir::Operand;
use rustc_middle::ty::{Ty, TyKind};
use rustc_span::{Span, source_map::Spanned};

use crate::TRAIT_METHOD_ACCOUNTS;
use crate::utils::account_extraction::extract_init_accounts_and_inner_types;
use crate::utils::mir_analysis::collect_account_field_assignments;
use crate::utils::name_resolution::check_if_init_account_self_method;
use crate::utils::types::InitAccountInfo;

use std::collections::{HashMap, HashSet};

/// Analyze a nested init-like function and return per-account field assignments.
pub fn analyze_nested_init_function<'tcx>(
    cx: &LateContext<'tcx>,
    fn_def_id: &DefId,
    parent_init_accounts: &HashMap<String, InitAccountInfo<'tcx>>,
    init_accounts_passed_to_nested_fn: Option<(String, InitAccountInfo<'tcx>)>,
    parent_fn_span: Span,
) -> HashMap<String, HashSet<String>> {
    let Some(local_def_id) = fn_def_id.as_local() else {
        return HashMap::new();
    };

    // Try to get HIR body
    let body_id = match get_hir_body_from_local_def_id(cx, local_def_id) {
        Some(body_id) => body_id,
        None => {
            // If it's a trait method, try to find the implementation
            // For trait methods, we can't easily find the implementation
            // without knowing the receiver type. Since we're analyzing a method call,
            // we should have the receiver type from the call site.
            // For now, skip trait methods - they're harder to analyze
            let hir_id = cx.tcx.local_def_id_to_hir_id(local_def_id);
            match cx.tcx.hir_node(hir_id) {
                rustc_hir::Node::TraitItem(_trait_item) => {
                    // Mark that this function uses trait methods
                    TRAIT_METHOD_ACCOUNTS.with(|trait_method_accounts| {
                        trait_method_accounts.borrow_mut().insert(parent_fn_span);
                    });
                    return HashMap::new();
                }
                _ => {
                    return HashMap::new();
                }
            }
        }
    };

    let body = cx.tcx.hir_body(body_id);
    let mut mir_analyzer = MirAnalyzer::new(cx, body, local_def_id);

    if mir_analyzer.anchor_context_info.is_none() {
        mir_analyzer.update_anchor_context_info_with_context_accounts(body);
    }
    let Some(anchor_context) = mir_analyzer.anchor_context_info.as_ref() else {
        return HashMap::new();
    };
    let mut init_accounts = extract_init_accounts_and_inner_types(cx, anchor_context);
    if init_accounts.is_empty() {
        if let Some(init_account) = init_accounts_passed_to_nested_fn {
            init_accounts.insert(init_account.0, init_account.1);
        } else {
            let Some(init_account) =
                check_if_init_account_self_method(anchor_context, parent_init_accounts)
            else {
                return HashMap::new();
            };
            init_accounts.insert(init_account.0, init_account.1);
        }
    }
    collect_account_field_assignments(
        cx,
        &mir_analyzer,
        local_def_id,
        &init_accounts,
        parent_fn_span,
    )
}

/// Check if function arguments correspond to init accounts.
pub fn check_if_args_corresponds_to_init_accounts<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    args: &[Spanned<Operand<'tcx>>],
    init_accounts: &HashMap<String, InitAccountInfo<'tcx>>,
) -> Option<(String, InitAccountInfo<'tcx>)> {
    for arg in args {
        let Some((_local, account_ty)) = mir_analyzer.extract_local_and_ty_from_operand(arg) else {
            continue;
        };
        // Normalize Account/AccountLoader/&T → inner struct type T
        let Some(arg_inner_ty) = inner_struct_ty(mir_analyzer.cx, account_ty) else {
            continue;
        };
        for (account_name, info) in init_accounts {
            let info_inner = info.inner_ty.peel_refs();
            let arg_inner = arg_inner_ty.peel_refs();
            match (info_inner.kind(), arg_inner.kind()) {
                (TyKind::Adt(info_adt, _), TyKind::Adt(arg_adt, _))
                    if info_adt.did() == arg_adt.did() =>
                {
                    return Some((account_name.clone(), info.clone()));
                }
                _ => {}
            }
        }
    }
    None
}

/// Extract the inner struct type from Account/AccountLoader wrappers.
fn inner_struct_ty<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, substs) = ty.kind() {
        let path = cx.tcx.def_path_str(adt_def.did());

        if (path.contains("anchor_lang::prelude::Account")
            || path.ends_with("anchor_lang::accounts::account::Account"))
            && let Some(inner) = substs.types().next()
        {
            return Some(inner);
        }

        if path.contains("anchor_lang::prelude::AccountLoader")
            && let Some(inner) = substs.types().next()
        {
            return Some(inner);
        }
    }
    Some(ty)
}

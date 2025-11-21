use rustc_hir::{BodyId, ImplItemKind, ItemKind, Node, def_id::DefId};
use rustc_lint::LateContext;
use rustc_middle::mir::{BasicBlock, Body as MirBody, Local};

use std::collections::{HashMap, HashSet};

use crate::models::*;
use crate::utils::{reachable_block, resolve_to_original_local};

// Processes nested function blocks and adds them to account_reloads or account_accesses
pub fn process_nested_function_blocks<'tcx>(
    nested_function_blocks: Vec<NestedFunctionBlocks<'tcx>>,
    nested_argument: &NestedArgument<'tcx>,
    anchor_context_info: &AnchorContextInfo<'tcx>,
    bb: BasicBlock,
    account_reloads: &mut HashMap<String, HashSet<BasicBlock>>,
    account_accesses: &mut HashMap<String, Vec<AccountAccess>>,
) {
    for nested_function_block in nested_function_blocks.into_iter() {
        if nested_argument.arg_type == NestedArgumentType::Account {
            for (nested_account_name, nested_account) in
                nested_argument.accounts.clone().into_iter()
            {
                if nested_account.account_ty == nested_function_block.account_ty
                    && nested_account.account_local == nested_function_block.account_local
                {
                    let account_block_name = format!(
                        "{}.accounts.{}",
                        anchor_context_info.anchor_context_name, nested_account_name
                    );
                    add_nested_function_block(
                        account_block_name,
                        &nested_function_block,
                        bb,
                        account_reloads,
                        account_accesses,
                    );
                }
            }
        } else {
            let account_block_name = format!(
                "{}.accounts.{}",
                anchor_context_info.anchor_context_name, nested_function_block.account_name
            );
            add_nested_function_block(
                account_block_name,
                &nested_function_block,
                bb,
                account_reloads,
                account_accesses,
            );
        }
    }
}

// Helper function to add nested function block to account_reloads or account_accesses
pub fn add_nested_function_block<'tcx>(
    account_block_name: String,
    nested_function_block: &NestedFunctionBlocks<'tcx>,
    bb: BasicBlock,
    account_reloads: &mut HashMap<String, HashSet<BasicBlock>>,
    account_accesses: &mut HashMap<String, Vec<AccountAccess>>,
) {
    if nested_function_block.block_type == NestedBlockType::Reload {
        if nested_function_block.not_used_reload {
            return;
        }
        account_reloads
            .entry(account_block_name)
            .or_default()
            .insert(bb);
    } else {
        account_accesses
            .entry(account_block_name)
            .or_default()
            .push(AccountAccess {
                access_block: bb,
                access_span: nested_function_block.account_span,
                stale_data_access: nested_function_block.stale_data_access,
            });
    }
}

// Creates a CpiContextCreationBlock with appropriate cpi_context_local
pub fn create_cpi_context_creation_block(
    account_name_and_local: AccountNameAndLocal,
    cpi_context_block: BasicBlock,
    _mir_body: &MirBody<'_>, // Add underscore prefix
    transitive_assignment_reverse_map: &HashMap<Local, Vec<Local>>,
) -> Option<CpiContextCreationBlock> {
    let arg_local = resolve_to_original_local(
        &account_name_and_local.account_local,
        &mut HashSet::new(),
        transitive_assignment_reverse_map,
    );

    Some(CpiContextCreationBlock {
        cpi_context_block,
        account_name: account_name_and_local.account_name,
        cpi_context_local: arg_local,
    })
}

// Processes nested CPI context creation and adds them to cpi_accounts
pub fn process_nested_cpi_context_creation<'tcx>(
    nested_cpi_context_creation: Vec<CpiContextCreationBlock>,
    nested_argument: &NestedArgument<'tcx>,
    anchor_context_info: &AnchorContextInfo<'tcx>,
    bb: BasicBlock,
    cpi_accounts: &mut HashMap<String, BasicBlock>,
) {
    for cpi_context_creation in nested_cpi_context_creation {
        if nested_argument.arg_type == NestedArgumentType::Account {
            for (nested_account_name, nested_account) in nested_argument.accounts.clone() {
                if nested_account.account_local == cpi_context_creation.cpi_context_local {
                    let account_block_name = format!(
                        "{}.accounts.{}",
                        anchor_context_info.anchor_context_name, nested_account_name
                    );
                    cpi_accounts.insert(account_block_name, bb);
                }
            }
        } else {
            let account_block_name = format!(
                "{}.accounts.{}",
                anchor_context_info.anchor_context_name, cpi_context_creation.account_name
            );
            cpi_accounts.insert(account_block_name, bb);
        }
    }
}

pub fn remap_nested_function_blocks<'tcx>(
    nested_blocks: Vec<NestedFunctionBlocks<'tcx>>,
    nested_argument: &NestedArgument<'tcx>,
    bb: BasicBlock,
) -> Vec<NestedFunctionBlocks<'tcx>> {
    nested_blocks
        .into_iter()
        .map(|mut nested_block| {
            nested_block.account_block = bb;
            if nested_argument.arg_type == NestedArgumentType::Account {
                for (nested_account_name, nested_account) in nested_argument.accounts.iter() {
                    if nested_account.account_ty == nested_block.account_ty
                        && nested_account.account_local == nested_block.account_local
                    {
                        nested_block.account_name = nested_account_name.clone();
                    }
                }
            }
            nested_block
        })
        .collect()
}

pub fn merge_nested_cpi_context_creation<'tcx>(
    nested_cpi_context_creation: Vec<CpiContextCreationBlock>,
    nested_argument: &NestedArgument<'tcx>,
    cpi_context_creation: &mut Vec<CpiContextCreationBlock>,
) {
    for nested_context in nested_cpi_context_creation {
        if nested_argument.arg_type == NestedArgumentType::Account {
            for (nested_account_name, nested_account) in nested_argument.accounts.iter() {
                if nested_account.account_local == nested_context.cpi_context_local {
                    cpi_context_creation.push(CpiContextCreationBlock {
                        cpi_context_block: nested_context.cpi_context_block,
                        account_name: nested_account_name.clone(),
                        cpi_context_local: nested_context.cpi_context_local,
                    });
                }
            }
        } else {
            cpi_context_creation.push(nested_context);
        }
    }
}

pub fn mark_unused_nested_reloads<'tcx>(
    mir_body: &MirBody<'tcx>,
    nested_function_blocks: &mut [NestedFunctionBlocks<'tcx>],
    cpi_calls: &[CpiCallBlock],
) {
    for reload in nested_function_blocks.iter_mut() {
        if reload.block_type != NestedBlockType::Reload {
            continue;
        }
        reload.not_used_reload = !cpi_calls.iter().any(|cpi_call| {
            reachable_block(
                &mir_body.basic_blocks,
                cpi_call.cpi_call_block,
                reload.account_block,
            )
        });
    }
}

// Checks if a data access is stale by checking if it is reachable from a reload.
pub fn check_stale_data_accesses<'tcx>(
    mir: &MirBody<'tcx>,
    nested_function_blocks: &mut Vec<NestedFunctionBlocks<'tcx>>,
) {
    let nested_function_reloads: Vec<NestedFunctionBlocks<'tcx>> = nested_function_blocks
        .iter()
        .filter(|block| block.block_type == NestedBlockType::Reload)
        .cloned()
        .collect();
    if nested_function_reloads.is_empty() {
        return;
    }
    for nested_account_access_block in nested_function_blocks.iter_mut() {
        if nested_account_access_block.block_type == NestedBlockType::Reload {
            continue;
        }
        let mut is_rechable_from_reload = false;
        for nested_account_reload_block in nested_function_reloads.iter() {
            if nested_account_reload_block.block_type == NestedBlockType::Access {
                continue;
            }
            if nested_account_access_block.account_name == nested_account_reload_block.account_name
                && nested_account_access_block.account_ty == nested_account_reload_block.account_ty
                && nested_account_access_block.account_block
                    != nested_account_reload_block.account_block
                && reachable_block(
                    &mir.basic_blocks,
                    nested_account_reload_block.account_block,
                    nested_account_access_block.account_block,
                )
            {
                is_rechable_from_reload = true;
                break;
            }
        }
        nested_account_access_block.stale_data_access = !is_rechable_from_reload;
    }
}

pub fn get_nested_fn_arg_names<'tcx>(
    cx: &LateContext<'tcx>,
    fn_def_id: DefId,
) -> Vec<(usize, String)> {
    let mut result = Vec::new();

    // Only functions defined in the same crate have HIR bodies.
    if !fn_def_id.is_local() {
        return result;
    }

    let hir_id = cx.tcx.local_def_id_to_hir_id(fn_def_id.expect_local());
    if let Node::Item(item) = cx.tcx.hir_node(hir_id) {
        if let ItemKind::Fn { body, .. } = &item.kind {
            collect_fn_args(cx, *body, &mut result);
        }
    } else if let Node::ImplItem(impl_item) = cx.tcx.hir_node(hir_id)
        && let ImplItemKind::Fn(_fn_sig, body_id) = &impl_item.kind
    {
        collect_fn_args(cx, *body_id, &mut result);
    }

    result
}

fn collect_fn_args<'tcx>(cx: &LateContext<'tcx>, body_id: BodyId, out: &mut Vec<(usize, String)>) {
    let body = cx.tcx.hir_body(body_id);
    for (idx, param) in body.params.iter().enumerate() {
        let name = match param.pat.kind {
            rustc_hir::PatKind::Binding(_, _, ident, _) => ident.name.to_string(),
            _ => format!("arg_{idx}"),
        };
        out.push((idx, name));
    }
}

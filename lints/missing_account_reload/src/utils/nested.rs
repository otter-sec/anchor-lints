use anchor_lints_utils::mir_analyzer::{AnchorContextInfo, MirAnalyzer};
use anchor_lints_utils::models::{AccountNameAndLocal, NestedArgument, NestedArgumentType};
use rustc_hir::{BodyId, ImplItemKind, ItemKind, Node, def_id::DefId};
use rustc_lint::LateContext;
use rustc_middle::mir::{BasicBlock, Body as MirBody, HasLocalDecls, Local, Operand};
use rustc_span::Span;
use rustc_span::source_map::Spanned;

use std::collections::{HashMap, HashSet};

use crate::utils::{contains_deserialized_data, extract_account_name_from_local, reachable_block};
use crate::{analyze_nested_function_operations, models::*};

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
pub fn create_cpi_context_creation_block<'tcx>(
    account_name_and_local: AccountNameAndLocal,
    cpi_context_block: BasicBlock,
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
) -> Option<CpiContextCreationBlock> {
    let arg_local = mir_analyzer
        .resolve_to_original_local(account_name_and_local.account_local, &mut HashSet::new());

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
            for (nested_account_name, nested_account) in nested_argument.accounts.iter() {
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

// Handle Account::reload calls
pub fn handle_account_reload_in_nested_function<'tcx>(
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
    mir_body: &MirBody<'tcx>,
    args: &[Spanned<Operand>],
    fn_span: Span,
    bb: BasicBlock,
) -> Option<NestedFunctionBlocks<'tcx>> {
    let account = args.first()?;
    let Operand::Move(account) = account.node else {
        return None;
    };
    let local = account.as_local()?;
    let account_ty = mir_body.local_decls().get(local)?.ty.peel_refs();

    let account_name_and_local = extract_account_name_from_local(mir_analyzer, &local, true)?;
    let arg_local = mir_analyzer
        .resolve_to_original_local(account_name_and_local.account_local, &mut HashSet::new());

    Some(NestedFunctionBlocks {
        account_name: account_name_and_local.account_name.clone(),
        account_ty,
        account_local: arg_local,
        account_span: fn_span,
        account_block: bb,
        stale_data_access: false,
        block_type: NestedBlockType::Reload,
        not_used_reload: false,
    })
}

// Handle account access (deref method)
pub fn handle_account_access_in_nested_function<'tcx>(
    cx: &LateContext<'tcx>,
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
    mir_body: &MirBody<'tcx>,
    args: &[Spanned<Operand>],
    fn_span: Span,
    bb: BasicBlock,
) -> Vec<NestedFunctionBlocks<'tcx>> {
    let mut blocks = Vec::new();

    for account in args {
        let Operand::Move(account) = account.node else {
            continue;
        };
        let Some(local) = account.as_local() else {
            continue;
        };
        let Some(account_ty) = mir_body.local_decls().get(local).map(|d| d.ty.peel_refs()) else {
            continue;
        };

        let account_name_and_locals = mir_analyzer.check_local_and_assignment_locals(
            &local,
            &mut HashSet::new(),
            true,
            &mut String::new(),
        );

        for account_name_and_local in account_name_and_locals {
            if !contains_deserialized_data(cx, account_ty) {
                continue;
            }

            let arg_local = mir_analyzer.resolve_to_original_local(
                account_name_and_local.account_local,
                &mut HashSet::new(),
            );
            blocks.push(NestedFunctionBlocks {
                account_name: account_name_and_local.account_name,
                account_ty,
                account_local: arg_local,
                account_span: fn_span,
                account_block: bb,
                stale_data_access: false,
                block_type: NestedBlockType::Access,
                not_used_reload: false,
            });
        }
    }

    blocks
}

// Handle CPI invoke or takes_cpi_context
pub fn handle_cpi_invoke_in_nested_function<'tcx>(
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
    args: &[Spanned<Operand<'tcx>>],
    fn_span: Span,
    bb: BasicBlock,
    arg_names: &[(usize, String)],
) -> (CpiCallBlock, Vec<CpiContextCreationBlock>) {
    let cpi_call = CpiCallBlock {
        cpi_call_block: bb,
        cpi_call_span: fn_span,
    };

    let mut cpi_context_creation = Vec::new();
    if let Some(account_infos_arg) = args.get(1) {
        for account in mir_analyzer.collect_accounts_from_account_infos_arg(account_infos_arg, true)
        {
            let mut arg_local = account.account_local;
            for (idx, name) in arg_names.iter() {
                if name == &account.account_name {
                    arg_local = Local::from_usize(*idx + 1);
                    break;
                }
            }
            cpi_context_creation.push(CpiContextCreationBlock {
                cpi_context_block: bb,
                account_name: account.account_name,
                cpi_context_local: arg_local,
            });
        }
    }

    (cpi_call, cpi_context_creation)
}

// Handle CPI context creation
pub fn handle_cpi_context_creation_in_nested_function<'tcx>(
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
    args: &[Spanned<Operand>],
    bb: BasicBlock,
) -> Vec<CpiContextCreationBlock> {
    let mut cpi_context_creation = Vec::new();

    let Some(cpi_accounts_struct) = args.get(1) else {
        return cpi_context_creation;
    };
    let (Operand::Copy(place) | Operand::Move(place)) = &cpi_accounts_struct.node else {
        return cpi_context_creation;
    };
    let Some(accounts_local) = place.as_local() else {
        return cpi_context_creation;
    };
    let Some(accounts) =
        mir_analyzer.find_cpi_accounts_struct(&accounts_local, &mut HashSet::new())
    else {
        return cpi_context_creation;
    };

    for account_local in accounts {
        if let Some(account_name_and_local) =
            extract_account_name_from_local(mir_analyzer, &account_local, true)
            && let Some(cpi_context_block) =
                create_cpi_context_creation_block(account_name_and_local.clone(), bb, mir_analyzer)
        {
            cpi_context_creation.push(cpi_context_block);
        }
    }

    cpi_context_creation
}

// Handle nested function calls
pub fn handle_nested_function_call<'tcx>(
    cx: &LateContext<'tcx>,
    def_id: DefId,
    fn_crate_name: &String,
    cpi_context_info: &AnchorContextInfo<'tcx>,
    bb: BasicBlock,
    nested_argument: &NestedArgument<'tcx>,
) -> (
    Vec<NestedFunctionBlocks<'tcx>>,
    Vec<CpiCallBlock>,
    Vec<CpiContextCreationBlock>,
) {
    let nested_function_operations =
        analyze_nested_function_operations(cx, &def_id, fn_crate_name, cpi_context_info);

    let nested_blocks = remap_nested_function_blocks(
        nested_function_operations.nested_function_blocks,
        nested_argument,
        bb,
    );

    let mut cpi_context_creation = Vec::new();
    merge_nested_cpi_context_creation(
        nested_function_operations.cpi_context_creation,
        nested_argument,
        &mut cpi_context_creation,
    );

    let mut cpi_calls = Vec::new();
    for cpi_call in nested_function_operations.cpi_calls {
        cpi_calls.push(CpiCallBlock {
            cpi_call_block: bb,
            cpi_call_span: cpi_call.cpi_call_span,
        });
    }

    (nested_blocks, cpi_calls, cpi_context_creation)
}

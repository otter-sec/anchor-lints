use anchor_lints_utils::{
    diag_items::{is_anchor_cpi_context, is_anchor_cpi_context_with_remaining_accounts_fn},
    mir_analyzer::{AnchorContextInfo, MirAnalyzer},
    utils::{check_locals_are_related, extract_arg_local},
};

use clippy_utils::source::HasSession;

use rustc_hir::def_id::DefId;
use rustc_lint::LateContext;
use rustc_middle::{
    mir::{
        AggregateKind, BasicBlock, Local, Operand, Place, Rvalue, StatementKind, TerminatorKind,
    },
    ty::{self as rustc_ty},
};
use rustc_span::{Span, source_map::Spanned};

use std::collections::{HashSet, VecDeque};

pub fn is_remaining_accounts_method(cx: &LateContext, fn_def_id: DefId) -> bool {
    is_anchor_cpi_context_with_remaining_accounts_fn(cx.tcx, fn_def_id)
}

#[derive(Debug, Clone)]
pub struct LamportMutation {
    pub span: Span,
    pub block: BasicBlock,
}

#[derive(Debug, Clone)]
pub struct CpiCallInfo {
    pub block: BasicBlock,
    pub span: Span,
    pub accounts: HashSet<String>,
}

pub fn detect_lamport_mutation<'cx, 'tcx>(
    cx: &'cx LateContext<'tcx>,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    place: &Place<'tcx>,
    rvalue: &Rvalue<'tcx>,
    _anchor_context_info: &AnchorContextInfo<'tcx>,
) -> Option<String> {
    // Check if this is a method call to borrow_mut on lamports
    if let Some(local) = place.as_local()
        && let Some(span) = mir_analyzer.get_span_from_local(&local)
        && let Ok(snippet) = cx.sess().source_map().span_to_snippet(span)
        && snippet.contains("lamports")
        && snippet.contains("borrow_mut")
        && let Some(account_name) = extract_account_from_lamport_snippet(&snippet)
    {
        return Some(account_name);
    }

    // Check the rvalue for method calls
    if let Rvalue::Ref(_, _, place_ref) = rvalue
        && let Some(local) = place_ref.as_local()
    {
        // Check if this local is from a method call on lamports
        if let Some(receiver_local) = mir_analyzer.method_call_receiver_map.get(&local) {
            // Check if the receiver is an account with lamports field
            if let Some(account_info) =
                mir_analyzer.extract_account_name_from_local(receiver_local, true)
                && let Some(span) = mir_analyzer.get_span_from_local(&local)
                && let Ok(snippet) = cx.sess().source_map().span_to_snippet(span)
                && snippet.contains("lamports")
                && snippet.contains("borrow_mut")
            {
                return Some(account_info.account_name);
            }
        }
    }

    None
}

pub fn extract_account_from_lamport_snippet(snippet: &str) -> Option<String> {
    // Pattern: ctx.accounts.<name>.lamports.borrow_mut()
    if let Some(accounts_pos) = snippet.find(".accounts.") {
        let after_accounts = &snippet[accounts_pos + ".accounts.".len()..];
        if let Some(dot_pos) = after_accounts.find('.') {
            let account_name = &after_accounts[..dot_pos];
            if !account_name.is_empty() {
                return Some(account_name.to_string());
            }
        }
    }
    None
}

pub fn extract_cpi_accounts<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    args: &[Spanned<Operand<'tcx>>],
    cpi_block: BasicBlock,
) -> HashSet<String> {
    let mut accounts = HashSet::new();
    let Some(cpi_ctx_local) = extract_arg_local(args, 0) else {
        return HashSet::new();
    };

    accounts.extend(extract_cpi_accounts_from_context(
        mir_analyzer,
        cpi_block,
        cpi_ctx_local,
    ));
    accounts
}

pub fn extract_cpi_accounts_from_context(
    mir_analyzer: &MirAnalyzer,
    cpi_block: BasicBlock,
    cpi_ctx_local: Local,
) -> HashSet<String> {
    let mut accounts = HashSet::new();

    for (bb, bbdata) in mir_analyzer.mir.basic_blocks.iter_enumerated() {
        if let TerminatorKind::Call {
            func: Operand::Constant(func_const),
            args,
            destination,
            ..
        } = &bbdata.terminator().kind
            && let rustc_ty::FnDef(fn_def_id, _) = func_const.ty().kind()
        {
            let fn_sig = mir_analyzer.cx.tcx.fn_sig(*fn_def_id).skip_binder();
            let return_ty = fn_sig.skip_binder().output();

            // Check if the function return type is a CPI context
            if !is_anchor_cpi_context(mir_analyzer.cx.tcx, return_ty) {
                continue;
            }

            if let Some(cpi_accounts_local) = extract_arg_local(args, 1)
                && let Some(destination_local) = destination.as_local()
                && check_locals_are_related(
                    &mir_analyzer.reverse_assignment_map,
                    &destination_local,
                    &cpi_ctx_local,
                )
            {
                // Check if the function is a remaining accounts method and extract the remaining accounts
                if is_remaining_accounts_method(mir_analyzer.cx, *fn_def_id) {
                    if let Some(cpi_context_local) = extract_arg_local(args, 0)
                        && let Some(destination_local) = destination.as_local()
                        && check_locals_are_related(
                            &mir_analyzer.reverse_assignment_map,
                            &destination_local,
                            &cpi_ctx_local,
                        )
                    {
                        let remaining_accounts =
                            extract_cpi_accounts_from_context(mir_analyzer, bb, cpi_context_local);
                        accounts.extend(remaining_accounts);
                        // Second argument is the vec of remaining accounts
                        if let Some(vec_arg) = args.get(1) {
                            let vec_accounts =
                                mir_analyzer.collect_accounts_from_account_infos_arg(vec_arg, true);
                            if !vec_accounts.is_empty() {
                                for account in vec_accounts {
                                    accounts.insert(account.account_name);
                                }
                            } else {
                                accounts.extend(extract_accounts_from_vec_by_tracing(
                                    mir_analyzer,
                                    cpi_context_local,
                                ));
                            }
                        }
                    }
                } else {
                    accounts.extend(extract_cpi_accounts_from_account_struct(
                        mir_analyzer,
                        bb,
                        cpi_accounts_local,
                    ));
                }
                break;
            }
        }

        // Stop iterating if we've reached the CPI block
        if cpi_block == bb {
            break;
        }
    }
    accounts
}

pub fn extract_cpi_accounts_from_account_struct(
    mir_analyzer: &MirAnalyzer,
    cpi_accounts_block: BasicBlock,
    cpi_accounts_local: Local,
) -> HashSet<String> {
    let mut accounts = HashSet::new();

    // Find the CPI accounts struct
    for (bb, bbdata) in mir_analyzer.mir.basic_blocks.iter_enumerated() {
        if let TerminatorKind::Call { .. } = &bbdata.terminator().kind {
            for stmt in &bbdata.statements {
                if let StatementKind::Assign(box (place, rvalue)) = &stmt.kind
                    && let Rvalue::Aggregate(box AggregateKind::Adt(_, _, _, _, _), fields) = rvalue
                {
                    let Some(account_struct_local) = place.as_local() else {
                        continue;
                    };

                    // Check if the account struct local is related to the CPI accounts local
                    if !check_locals_are_related(
                        &mir_analyzer.reverse_assignment_map,
                        &account_struct_local,
                        &cpi_accounts_local,
                    ) {
                        continue;
                    }

                    for field_operand in fields.iter() {
                        if let Operand::Copy(p) | Operand::Move(p) = field_operand
                            && let Some(local) = p.as_local()
                            && let Some(account_info) =
                                mir_analyzer.extract_account_name_from_local(&local, true)
                        {
                            accounts.insert(account_info.account_name.clone());
                        }
                    }

                    break;
                }
            }
        }
        // Stop if we've reached the CPI accounts block
        if cpi_accounts_block == bb {
            break;
        }
    }
    accounts
}

pub fn is_reachable<'tcx>(
    mir: &rustc_middle::mir::Body<'tcx>,
    from: BasicBlock,
    to: BasicBlock,
) -> bool {
    if from == to {
        return true;
    }

    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    visited.insert(from);
    queue.push_back(from);

    while let Some(current) = queue.pop_front() {
        if current == to {
            return true;
        }

        // Get successors from the terminator
        if let Some(terminator) = &mir.basic_blocks[current].terminator {
            for succ in terminator.successors() {
                if visited.insert(succ) {
                    queue.push_back(succ);
                }
            }
        }
    }

    false
}

pub fn extract_accounts_from_vec_by_tracing(
    mir_analyzer: &MirAnalyzer,
    vec_local: Local,
) -> HashSet<String> {
    let mut accounts = HashSet::new();
    let vec_accounts = mir_analyzer.get_vec_elements(&vec_local, &mut HashSet::new(), true);
    for account in vec_accounts {
        accounts.insert(account.account_name);
    }
    accounts
}

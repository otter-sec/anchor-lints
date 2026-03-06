use anchor_lints_utils::{
    diag_items::is_solana_instruction_type,
    mir_analyzer::MirAnalyzer,
    models::{NestedArgument, NestedArgumentType, ParamInfo},
};
use rustc_middle::{
    mir::{
        BasicBlock, BasicBlocks, HasLocalDecls, Local, Operand, Place, Rvalue, Statement,
        StatementKind,
    },
    ty::{self as rustc_ty},
};
use rustc_span::{Span, source_map::Spanned};

use std::collections::{HashMap, HashSet, VecDeque};

use crate::{
    models::{Cmp, CpiCallsInfo, CpiContextsInfo, IfThen},
    pubkey_checked_in_this_block,
};
use anchor_lints_utils::models::{AssignmentKind, Origin};

pub fn get_local_from_operand<'tcx>(operand: Option<&Spanned<Operand<'tcx>>>) -> Option<Local> {
    operand.and_then(|op| match &op.node {
        Operand::Copy(place) | Operand::Move(place) => place.as_local(),
        Operand::Constant(_) => None,
    })
}

pub fn check_program_id_included_in_conditional_blocks<'tcx>(
    cpi_ctx_local: &Local,
    cmps: &[Cmp],
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
) -> bool {
    let mut cpi_context_references: HashSet<Local> = HashSet::new();
    cpi_context_references.insert(*cpi_ctx_local);

    for (k, v) in &mir_analyzer.transitive_assignment_reverse_map {
        if v.contains(cpi_ctx_local) || k == cpi_ctx_local {
            cpi_context_references.insert(*k);
            cpi_context_references.extend(v.iter().copied());
        }
    }

    cmps.iter().any(|cmp| {
        cpi_context_references.contains(&cmp.lhs)
            || cpi_context_references.contains(&cmp.rhs)
            || mir_analyzer.are_same_account(cmp.lhs, *cpi_ctx_local)
            || mir_analyzer.are_same_account(cmp.rhs, *cpi_ctx_local)
    })
}

pub fn cpi_invocation_is_reachable_from_cpi_context(
    graph: &BasicBlocks,
    from: BasicBlock,
    to: &HashMap<BasicBlock, CpiCallsInfo>,
) -> Option<BasicBlock> {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    visited.insert(from);
    queue.push_back(from);

    while let Some(u) = queue.pop_front() {
        if let Some(terminator) = &graph[u].terminator {
            for succ in terminator.successors() {
                if visited.contains(&succ) {
                    continue;
                }
                if to.contains_key(&succ) {
                    return Some(succ);
                }
                visited.insert(succ);
                queue.push_back(succ);
            }
        }
    }
    None
}

pub fn record_instruction_creation<'tcx>(
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
    bb: BasicBlock,
    statement: &Statement<'tcx>,
    instruction_to_program_id: &mut HashMap<Local, BasicBlock>,
) {
    if let StatementKind::Assign(box (place, rvalue)) = &statement.kind
        && let Some(dest_local) = place.as_local()
        && let Rvalue::Aggregate(_, operands) = rvalue
        && let Some(decl) = mir_analyzer.mir.local_decls().get(dest_local)
        && is_instruction_type(&mir_analyzer.cx.tcx, decl.ty.peel_refs())
        && let Some(first_operand) = operands.iter().next()
        && let Operand::Copy(place) | Operand::Move(place) = first_operand
        && let Some(program_id_local) = place.as_local()
        && mir_analyzer.is_pubkey_type(program_id_local)
    {
        instruction_to_program_id.insert(dest_local, bb);
    }
}

fn is_instruction_type<'tcx>(tcx: &rustc_ty::TyCtxt<'tcx>, ty: rustc_ty::Ty<'tcx>) -> bool {
    is_solana_instruction_type(*tcx, ty)
}

pub fn track_instruction_call<'tcx>(
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
    instruction_local: Local,
    fn_span: Span,
    bb: BasicBlock,
    cpi_calls: &mut HashMap<BasicBlock, CpiCallsInfo>,
    cpi_contexts: &mut HashMap<BasicBlock, CpiContextsInfo>,
    instruction_to_program_id: &HashMap<Local, BasicBlock>,
) {
    let mir = mir_analyzer.mir;
    let decl_ty = match mir
        .local_decls()
        .get(instruction_local)
        .map(|d| d.ty.peel_refs())
    {
        Some(ty) => ty,
        None => return,
    };

    if !is_instruction_type(&mir_analyzer.cx.tcx, decl_ty) {
        return;
    }

    let mut program_id_local = None;
    let mut program_id_bb = None;

    if let Some(&pid) = instruction_to_program_id.get(&instruction_local) {
        program_id_local = Some(instruction_local);
        program_id_bb = Some(pid);
    } else {
        let mut to_check = vec![instruction_local];
        let mut visited = HashSet::new();

        while let Some(current) = to_check.pop() {
            if !visited.insert(current) {
                continue;
            }

            if let Some(&pid) = instruction_to_program_id.get(&current) {
                program_id_local = Some(instruction_local);
                program_id_bb = Some(pid);
                break;
            }

            for (source_key, destinations) in &mir_analyzer.transitive_assignment_reverse_map {
                if destinations.contains(&current) {
                    to_check.push(*source_key);
                } else if source_key == &current {
                    to_check.extend(destinations);
                }
            }

            if let Some(AssignmentKind::FromPlace(src_place)) =
                mir_analyzer.assignment_map.get(&current)
                && let Some(src_local) = src_place.as_local()
            {
                to_check.push(src_local);
            }
        }
    }

    let (Some(pid_local), Some(pid_bb)) = (program_id_local, program_id_bb) else {
        return;
    };

    let origin = mir_analyzer.origin_of_operand(&Operand::Copy(Place::from(pid_local)));
    if matches!(origin, Origin::Parameter | Origin::Unknown) {
        cpi_calls.insert(
            bb,
            CpiCallsInfo {
                span: fn_span,
                local: instruction_local,
            },
        );

        cpi_contexts.insert(
            pid_bb,
            CpiContextsInfo {
                cpi_ctx_local: instruction_local,
                program_id_local: pid_local,
            },
        );
    }
}

pub fn map_nested_arg_accounts_to_account_cmps(
    nested_arg_accounts: &NestedArgument,
    param_info: &[ParamInfo],
    account_cmps: &mut [String],
) -> Vec<String> {
    if nested_arg_accounts.arg_type != NestedArgumentType::Account {
        return account_cmps.to_vec();
    }

    let local_to_param_name: HashMap<Local, &String> = param_info
        .iter()
        .map(|param| (param.param_local, &param.param_name))
        .collect();

    let account_cmps_set: HashSet<String> = account_cmps.iter().cloned().collect();

    let mut replacements: HashMap<String, String> = HashMap::new();
    for (account_name, account) in nested_arg_accounts.accounts.iter() {
        if account_cmps_set.contains(account_name)
            && let Some(param_name) = local_to_param_name.get(&account.account_local)
        {
            replacements.insert(account_name.clone(), param_name.to_string());
        }
    }

    for cmp_account in account_cmps.iter_mut() {
        if let Some(replacement) = replacements.get(cmp_account) {
            *cmp_account = replacement.clone();
        }
    }

    account_cmps.to_vec()
}

pub fn map_param_info_to_nested_accounts(
    nested_arg_accounts: &NestedArgument,
    param_info: &[ParamInfo],
    account_cmps: &mut [String],
) -> Vec<String> {
    if nested_arg_accounts.arg_type != NestedArgumentType::Account {
        return account_cmps.to_vec();
    }

    let local_to_account_name: HashMap<Local, &String> = nested_arg_accounts
        .accounts
        .iter()
        .map(|(account_name, account)| (account.account_local, account_name))
        .collect();

    let account_cmps_set: HashSet<String> = account_cmps.iter().cloned().collect();

    let mut replacements: HashMap<String, String> = HashMap::new();
    for param in param_info {
        if account_cmps_set.contains(&param.param_name)
            && let Some(account_name) = local_to_account_name.get(&param.param_local)
        {
            replacements.insert(param.param_name.clone(), account_name.to_string());
        }
    }

    for cmp_account in account_cmps.iter_mut() {
        if let Some(replacement) = replacements.get(cmp_account) {
            *cmp_account = replacement.clone();
        }
    }

    account_cmps.to_vec()
}

pub fn add_account_or_param_from_local<'tcx>(
    local: Local,
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
    existing_account_cmps: &mut Vec<String>,
) {
    if let Some(account) = mir_analyzer.is_from_cpi_context(local, None)
        && !existing_account_cmps.contains(&account.account_name)
    {
        existing_account_cmps.push(account.account_name);
    } else if let Some(param) = mir_analyzer.check_local_is_param(local)
        && !existing_account_cmps.contains(&param.param_name)
    {
        existing_account_cmps.push(param.param_name.clone());
    }
}

pub fn add_program_id_to_existing_account_cmps<'tcx>(
    program_id_cmps: &[Cmp],
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
    existing_account_cmps: &mut Vec<String>,
) {
    for cmp in program_id_cmps {
        add_account_or_param_from_local(cmp.lhs, mir_analyzer, existing_account_cmps);
        add_account_or_param_from_local(cmp.rhs, mir_analyzer, existing_account_cmps);
    }
}

pub fn is_account_checked_in_previous_blocks<'tcx>(
    program_id: &Local,
    existing_account_cmps: &[String],
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
) -> bool {
    if let Some(account) = mir_analyzer.is_from_cpi_context(*program_id, None)
        && existing_account_cmps.contains(&account.account_name)
    {
        return true;
    } else if let Some(param) = mir_analyzer.check_local_is_param(*program_id) {
        return existing_account_cmps.contains(&param.param_name);
    }
    false
}

pub fn filter_program_id_cmps<'tcx>(
    bb: BasicBlock,
    program_id_cmps: &[Cmp],
    switches: &[IfThen],
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
) -> Vec<Cmp> {
    let mut filtered_program_id_cmps = Vec::new();
    for cmp in program_id_cmps {
        let is_lhs_reachable =
            pubkey_checked_in_this_block(bb, cmp.lhs, program_id_cmps, switches, mir_analyzer);
        let is_rhs_reachable =
            pubkey_checked_in_this_block(bb, cmp.rhs, program_id_cmps, switches, mir_analyzer);
        if !is_lhs_reachable || !is_rhs_reachable {
            filtered_program_id_cmps.push(*cmp); // Dereference since Cmp is Copy
        }
    }
    filtered_program_id_cmps
}

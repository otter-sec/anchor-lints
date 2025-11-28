use anchor_lints_utils::mir_analyzer::MirAnalyzer;
use rustc_middle::{
    mir::{
        BasicBlock, BasicBlocks, HasLocalDecls, Local, Operand, Place, Rvalue, Statement,
        StatementKind,
    },
    ty::{self as rustc_ty},
};
use rustc_span::{Span, source_map::Spanned};

use std::collections::{HashMap, HashSet, VecDeque};

use crate::models::{Cmp, CpiCallsInfo, CpiContextsInfo};
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
    if let rustc_ty::TyKind::Adt(adt_def, _) = ty.kind() {
        let def_path = tcx.def_path_str(adt_def.did());
        def_path == "solana_program::instruction::Instruction"
            || def_path.contains("instruction::Instruction")
    } else {
        false
    }
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

use rustc_middle::{
    mir::{
        Body as MirBody, HasLocalDecls, Local, Operand, Place, Rvalue, StatementKind,
        TerminatorKind,
    },
    ty::TyKind,
};
use rustc_span::source_map::Spanned;

use std::collections::{HashMap, HashSet, VecDeque};

use crate::{mir_analyzer::AnchorContextInfo, models::*, utils::compare_adt_def_ids};

/// Builds the analysis maps for the MIR body
pub fn build_mir_analysis_maps<'tcx>(mir: &MirBody<'tcx>) -> MirAnalysisMaps<'tcx> {
    let mut assignment_map: HashMap<Local, AssignmentKind<'tcx>> = HashMap::new();
    let mut reverse_assignment_map: HashMap<Local, Vec<Local>> = HashMap::new();
    let mut cpi_account_local_map: HashMap<Local, Vec<Local>> = HashMap::new();

    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        for statement in &bbdata.statements {
            if let StatementKind::Assign(box (dest_place, rvalue)) = &statement.kind
                && let Some(dest_local) = dest_place.as_local()
            {
                // AssignmentKind classification
                let kind = match rvalue {
                    Rvalue::Use(Operand::Constant(_)) => AssignmentKind::Const,
                    Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) => {
                        AssignmentKind::FromPlace(*src)
                    }
                    Rvalue::Ref(_, _, src_place) => AssignmentKind::RefTo(*src_place),
                    _ => AssignmentKind::Other,
                };
                assignment_map.insert(dest_local, kind);

                // Helper closure used for reverse mapping
                let mut record_mapping = |src_place: &Place<'tcx>| {
                    reverse_assignment_map
                        .entry(src_place.local)
                        .or_default()
                        .push(dest_local);
                };

                // CPI map only for Aggregates
                if let Rvalue::Aggregate(_, field_operands) = rvalue {
                    for operand in field_operands {
                        if let Operand::Copy(field_place) | Operand::Move(field_place) = operand
                            && let Some(field_local) = field_place.as_local()
                        {
                            cpi_account_local_map
                                .entry(dest_local)
                                .or_default()
                                .push(field_local);
                        }
                    }
                }
                // Reverse mapping for all rvalue types
                match rvalue {
                    Rvalue::Use(Operand::Copy(src) | Operand::Move(src) ) => record_mapping(src),
                    Rvalue::Ref(_, _, src) => record_mapping(src),
                    Rvalue::Cast(_, Operand::Copy(src) | Operand::Move(src), _) => {
                        record_mapping(src)
                    }
                    Rvalue::Aggregate(_, operands) => {
                        for operand in operands {
                            if let Operand::Copy(src) | Operand::Move(src) = operand {
                                record_mapping(src);
                            }
                        }
                    }
                    Rvalue::CopyForDeref(src) => record_mapping(src),
                    _ => {}
                }
            }
        }
    }

    MirAnalysisMaps {
        assignment_map,
        reverse_assignment_map,
        cpi_account_local_map,
    }
}

/// Build transitive reverse map from direct reverse map
pub fn build_transitive_reverse_map(
    direct_map: &HashMap<Local, Vec<Local>>,
) -> HashMap<Local, Vec<Local>> {
    let mut transitive_map: HashMap<Local, Vec<Local>> = HashMap::new();

    for (&src, dests) in direct_map {
        let mut visited = HashSet::new();
        let mut queue: VecDeque<Local> = VecDeque::from(dests.clone());

        while let Some(next) = queue.pop_front() {
            if visited.insert(next) {
                transitive_map.entry(src).or_default().push(next);

                if let Some(next_dests) = direct_map.get(&next) {
                    for &nd in next_dests {
                        queue.push_back(nd);
                    }
                }
            }
        }
    }

    for vec in transitive_map.values_mut() {
        vec.sort();
    }

    transitive_map
}

// Builds a map of method call destinations to their receivers.
pub fn build_method_call_receiver_map<'tcx>(mir: &MirBody<'tcx>) -> HashMap<Local, Local> {
    let mut method_call_map: HashMap<Local, Local> = HashMap::new();

    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        if let Some(terminator) = &bbdata.terminator
            && let TerminatorKind::Call {
                func: _,
                args,
                destination,
                ..
            } = &terminator.kind
            && let Some(receiver) = args.first()
            && let Operand::Copy(receiver_place) | Operand::Move(receiver_place) = &receiver.node
            && let Some(receiver_local) = receiver_place.as_local()
            && let dest_place = destination
            && let Some(dest_local) = dest_place.as_local()
        {
            method_call_map.insert(dest_local, receiver_local);
        }
    }

    method_call_map
}

/// Checks if the first argument of a function call is an implementation method
pub fn is_implementation_method<'tcx>(
    mir: &MirBody<'tcx>,
    args: &[Spanned<Operand<'tcx>>],
    anchor_context_info: &AnchorContextInfo<'tcx>,
) -> bool {
    args.first()
        .and_then(|arg| {
            if let Operand::Copy(place) | Operand::Move(place) = &arg.node {
                place.as_local().and_then(|local| {
                    mir.local_decls().get(local).map(|decl| {
                        let ty = decl.ty.peel_refs();
                        // Check if it's a reference type (could be &self)
                        if let TyKind::Ref(_, inner_ty, _) = ty.kind() {
                            let inner_ty = inner_ty.peel_refs();
                            compare_adt_def_ids(
                                inner_ty,
                                anchor_context_info.anchor_context_account_type,
                            )
                        } else {
                            compare_adt_def_ids(ty, anchor_context_info.anchor_context_account_type)
                        }
                    })
                })
            } else {
                None
            }
        })
        .unwrap_or(false)
}

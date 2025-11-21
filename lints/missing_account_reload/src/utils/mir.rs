use clippy_utils::{source::HasSession, ty::is_type_diagnostic_item};
use rustc_lint::LateContext;
use rustc_middle::{
    mir::{
        BasicBlock, BasicBlocks, Body as MirBody, HasLocalDecls, Local, Operand, Place, Rvalue,
        StatementKind, TerminatorKind,
    },
    ty::TyKind,
};
use rustc_span::{Span, Symbol, source_map::Spanned, sym};

use std::collections::{HashMap, HashSet, VecDeque};

use crate::models::AccountNameAndLocal;
use crate::utils::{
    extract_context_account, extract_vec_elements, extract_vec_snippet_from_span, remove_comments,
};

// Builds a map of local variables to the local variables they are assigned to.
pub fn build_local_relationship_maps<'tcx>(
    mir: &MirBody<'tcx>,
) -> (HashMap<Local, Vec<Local>>, HashMap<Local, Vec<Local>>) {
    let mut cpi_account_local_map: HashMap<Local, Vec<Local>> = HashMap::new();
    let mut reverse_assignment_map: HashMap<Local, Vec<Local>> = HashMap::new();

    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        for statement in &bbdata.statements {
            if let StatementKind::Assign(box (dest_place, rvalue)) = &statement.kind
                && let Some(dest_local) = dest_place.as_local()
            {
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

                let mut record_mapping = |src_place: &Place<'tcx>| {
                    let src_local = src_place.local;
                    reverse_assignment_map
                        .entry(src_local)
                        .or_default()
                        .push(dest_local);
                };

                match rvalue {
                    Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) => record_mapping(src),
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

    (cpi_account_local_map, reverse_assignment_map)
}

// Builds a transitive reverse map of local variables to the local variables they are assigned to.
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

// Resolves the original local from a local in a transitive assignment map.
pub fn resolve_to_original_local(
    from_local: &Local,
    visited: &mut HashSet<Local>,
    reverse_assignment_map: &HashMap<Local, Vec<Local>>,
) -> Local {
    if visited.contains(from_local) {
        return *from_local;
    }
    visited.insert(*from_local);

    for (src_local, dest_locals) in reverse_assignment_map {
        if dest_locals.contains(from_local) {
            return resolve_to_original_local(src_local, visited, reverse_assignment_map);
        }
    }

    *from_local
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

// Checks if a block is reachable from another block.
pub fn reachable_block(graph: &BasicBlocks, from: BasicBlock, to: BasicBlock) -> bool {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    visited.insert(from);
    queue.push_back(from);

    while let Some(u) = queue.pop_front() {
        if u == to {
            return true;
        }
        for succ in graph[u]
            .terminator
            .as_ref()
            .map(|t| t.successors().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            if visited.contains(&succ) {
                continue;
            }
            visited.insert(succ);
            queue.push_back(succ);
        }
    }
    false
}

// Checks if a HashSet of blocks is reachable from another block.
pub fn reachable_blocks(graph: &BasicBlocks, from: BasicBlock, to: &HashSet<BasicBlock>) -> bool {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    visited.insert(from);
    queue.push_back(from);

    while let Some(u) = queue.pop_front() {
        if to.contains(&u) {
            return true;
        }
        for succ in graph[u]
            .terminator
            .as_ref()
            .map(|t| t.successors().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            if visited.contains(&succ) {
                continue;
            }
            visited.insert(succ);
            queue.push_back(succ);
        }
    }
    false
}

/// Finds blocks in `to` that are reachable from `from` nodes without passing through `without` nodes
/// Returns a list of `to` nodes with the `from` node they are reachable from
pub fn reachable_without_passing(
    graph: &BasicBlocks,
    from: HashSet<BasicBlock>,
    to: HashSet<BasicBlock>,
    without: HashSet<BasicBlock>,
) -> Vec<(BasicBlock, BasicBlock)> {
    let mut queue = VecDeque::new();
    // Map of nodes to the `from` block they are reachable from
    let mut origin = HashMap::new();
    let mut visited = HashSet::new();

    for &f in &from {
        origin.insert(f, f);
        visited.insert(f);
        queue.push_back(f);
    }

    while let Some(u) = queue.pop_front() {
        if without.contains(&u) {
            continue;
        }
        for succ in graph[u]
            .terminator
            .as_ref()
            .map(|t| t.successors().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            if without.contains(&succ) || visited.contains(&succ) {
                continue;
            }
            origin.insert(succ, origin[&u]);
            visited.insert(succ);
            queue.push_back(succ);
        }
    }

    to.into_iter()
        .filter_map(|bb| origin.get(&bb).map(|o| (bb, *o)))
        .collect()
}

// Checks if the function arguments contains a CPI context.
pub fn takes_cpi_context(
    cx: &LateContext<'_>,
    mir: &MirBody<'_>,
    args: &[Spanned<Operand>],
) -> bool {
    args.iter().any(|arg| {
        if let Operand::Copy(place) | Operand::Move(place) = &arg.node
            && let Some(local) = place.as_local()
            && let Some(decl) = mir.local_decls().get(local)
        {
            is_type_diagnostic_item(cx, decl.ty.peel_refs(), Symbol::intern("AnchorCpiContext"))
        } else {
            false
        }
    })
}

pub fn get_vec_elements(
    cx: &LateContext<'_>,
    mir: &MirBody<'_>,
    local: &Local,
    visited_locals: &mut HashSet<Local>,
    reverse_assignment_map: &HashMap<Local, Vec<Local>>,
    method_call_receiver_map: &HashMap<Local, Local>,
    return_only_name: bool,
) -> Vec<AccountNameAndLocal> {
    let mut elements = Vec::new();
    if let Some(span) = get_span_from_local(mir, local) {
        if visited_locals.contains(local) {
            if let Some(method_call_receiver) = method_call_receiver_map.get(local) {
                return get_vec_elements(
                    cx,
                    mir,
                    method_call_receiver,
                    visited_locals,
                    reverse_assignment_map,
                    method_call_receiver_map,
                    return_only_name,
                );
            }
            return elements;
        }
        visited_locals.insert(*local);
        let mut cleaned_snippet = String::new();
        if let Some(full_vec) = extract_vec_snippet_from_span(cx, span) {
            cleaned_snippet = remove_comments(&full_vec);
        } else if let Ok(snippet) = cx.tcx.sess().source_map().span_to_snippet(span) {
            cleaned_snippet = remove_comments(&snippet);
        }
        for element in extract_vec_elements(&cleaned_snippet) {
            if let Some(account_name) = extract_context_account(&element, return_only_name) {
                elements.push(AccountNameAndLocal {
                    account_name,
                    account_local: *local,
                });
            }
        }
        if !elements.is_empty() {
            return elements;
        }
        let resolved_local =
            resolve_to_original_local(local, &mut HashSet::new(), reverse_assignment_map);
        return get_vec_elements(
            cx,
            mir,
            &resolved_local,
            visited_locals,
            reverse_assignment_map,
            method_call_receiver_map,
            return_only_name,
        );
    }

    elements
}

fn get_span_from_local(mir: &MirBody<'_>, local: &Local) -> Option<Span> {
    mir.local_decls().get(*local).map(|d| d.source_info.span)
}

// Collects the accounts from the account_infos argument.
pub fn collect_accounts_from_account_infos_arg<'tcx>(
    cx: &LateContext<'tcx>,
    mir: &MirBody<'tcx>,
    arg: &Spanned<Operand<'tcx>>,
    reverse_assignment_map: &HashMap<Local, Vec<Local>>,
    method_call_receiver_map: &HashMap<Local, Local>,
    return_only_name: bool,
) -> Vec<AccountNameAndLocal> {
    if let Operand::Copy(place) | Operand::Move(place) = arg.node
        && let Some(vec_local) = place.as_local()
        && let Some(vec_ty) = mir.local_decls().get(vec_local).map(|d| d.ty.peel_refs())
        && (is_type_diagnostic_item(cx, vec_ty, sym::Vec)
            || matches!(vec_ty.kind(), TyKind::Slice(_)))
    {
        return get_vec_elements(
            cx,
            mir,
            &vec_local,
            &mut HashSet::new(),
            reverse_assignment_map,
            method_call_receiver_map,
            return_only_name,
        );
    }
    Vec::new()
}

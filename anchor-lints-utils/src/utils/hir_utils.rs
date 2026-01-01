use rustc_hir::{ImplItemKind, ItemKind, Node};
use rustc_lint::LateContext;

use rustc_middle::mir::Local;

/// Get HIR body from a LocalDefId, handling both Item and ImplItem cases
pub fn get_hir_body_from_local_def_id<'tcx>(
    cx: &LateContext<'tcx>,
    local_def_id: rustc_hir::def_id::LocalDefId,
) -> Option<rustc_hir::BodyId> {
    let hir_id = cx.tcx.local_def_id_to_hir_id(local_def_id);
    match cx.tcx.hir_node(hir_id) {
        Node::Item(item) => {
            if let ItemKind::Fn { body, .. } = &item.kind {
                Some(*body)
            } else {
                None
            }
        }
        Node::ImplItem(impl_item) => {
            if let ImplItemKind::Fn(_, body_id) = &impl_item.kind {
                Some(*body_id)
            } else {
                None
            }
        }
        _ => None,
    }
}

// Helper to check if two locals are related (same or one is derived from the other)
pub fn check_locals_are_related(
    reverse_assignment_map: &std::collections::HashMap<Local, Vec<Local>>,
    local1: &Local,
    local2: &Local,
) -> bool {
    use std::collections::HashSet;

    if local1 == local2 {
        return true;
    }

    let mut visited = HashSet::new();
    let mut to_check = vec![*local1];

    while let Some(current) = to_check.pop() {
        if visited.contains(&current) {
            continue;
        }
        visited.insert(current);

        if current == *local2 {
            return true;
        }

        // Check if current is derived from local2 (or vice versa)
        if let Some(derived) = reverse_assignment_map.get(&current) {
            to_check.extend(derived.iter().copied());
        }
    }

    // Also check the reverse direction
    let mut visited2 = HashSet::new();
    let mut to_check2 = vec![*local2];

    while let Some(current) = to_check2.pop() {
        if visited2.contains(&current) {
            continue;
        }
        visited2.insert(current);

        if current == *local1 {
            return true;
        }

        if let Some(derived) = reverse_assignment_map.get(&current) {
            to_check2.extend(derived.iter().copied());
        }
    }

    false
}

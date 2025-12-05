use rustc_hir::{ImplItemKind, ItemKind, Node};
use rustc_lint::LateContext;

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

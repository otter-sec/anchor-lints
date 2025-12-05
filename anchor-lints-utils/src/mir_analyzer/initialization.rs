use rustc_hir::{Body as HirBody, def_id::LocalDefId};
use rustc_lint::LateContext;

use super::types::MirAnalyzer;
use crate::utils::*;

impl<'cx, 'tcx> MirAnalyzer<'cx, 'tcx> {
    /// Create a new MirAnalyzer with all common initialization
    pub fn new(cx: &'cx LateContext<'tcx>, body: &'cx HirBody<'tcx>, def_id: LocalDefId) -> Self {
        // Get MIR
        let mir = cx.tcx.optimized_mir(def_id.to_def_id());

        // Build assignment maps
        let mir_analysis_maps = build_mir_analysis_maps(mir);
        let transitive_assignment_reverse_map =
            build_transitive_reverse_map(&mir_analysis_maps.reverse_assignment_map);
        let method_call_receiver_map = build_method_call_receiver_map(mir);

        let dominators = mir.basic_blocks.dominators();

        // Get anchor context info (optional - some lints may not need it)
        let anchor_context_info = get_anchor_context_accounts(cx, mir, body);

        Self {
            cx,
            mir,
            assignment_map: mir_analysis_maps.assignment_map,
            reverse_assignment_map: mir_analysis_maps.reverse_assignment_map,
            cpi_account_local_map: mir_analysis_maps.cpi_account_local_map,
            transitive_assignment_reverse_map,
            method_call_receiver_map,
            anchor_context_info,
            dominators: dominators.clone(),
            param_info: get_param_info(cx, mir, body),
        }
    }

    // Updates the anchor context info with the accounts
    pub fn update_anchor_context_info_with_context_accounts(&mut self, body: &HirBody<'tcx>) {
        let context_accounts = get_context_accounts(self.cx, self.mir, body);
        if let Some(context_accounts) = context_accounts {
            self.anchor_context_info = Some(context_accounts);
        }
    }
}

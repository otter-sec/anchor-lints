use rustc_data_structures::graph::dominators::Dominators;
use rustc_lint::LateContext;
use rustc_middle::mir::Local;
use rustc_middle::{
    mir::{BasicBlock, Body as MirBody},
    ty::Ty,
};

use std::collections::HashMap;

use crate::models::*;

#[derive(Debug, Clone)]
pub struct AnchorContextInfo<'tcx> {
    pub anchor_context_name: String,
    pub anchor_context_type: Ty<'tcx>,
    pub anchor_context_account_type: Ty<'tcx>,
    pub anchor_context_arg_accounts_type: HashMap<String, Ty<'tcx>>,
    #[allow(unused)]
    pub anchor_context_arg_local: Local,
}

/// Main analyzer struct that encapsulates common MIR analysis state and utilities
pub struct MirAnalyzer<'cx, 'tcx> {
    pub cx: &'cx LateContext<'tcx>,
    pub mir: &'cx MirBody<'tcx>,

    // Assignment maps
    pub assignment_map: HashMap<Local, AssignmentKind<'tcx>>,
    pub cpi_account_local_map: HashMap<Local, Vec<Local>>,
    pub reverse_assignment_map: HashMap<Local, Vec<Local>>,
    pub transitive_assignment_reverse_map: HashMap<Local, Vec<Local>>,
    pub method_call_receiver_map: HashMap<Local, Local>,

    pub dominators: Dominators<BasicBlock>,

    // Optional anchor context info (if function takes Anchor context)
    pub anchor_context_info: Option<AnchorContextInfo<'tcx>>,

    pub param_info: Vec<ParamInfo<'tcx>>,
}

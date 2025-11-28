use rustc_middle::mir::Local;
use rustc_middle::mir::Place;
use std::collections::HashMap;

#[derive(Debug)]
pub struct CpiAccountInfo {
    pub account_name: String,
    pub account_local: Local,
}

pub struct MirAnalysisMaps<'tcx> {
    pub assignment_map: HashMap<Local, AssignmentKind<'tcx>>,
    pub reverse_assignment_map: HashMap<Local, Vec<Local>>,
    pub cpi_account_local_map: HashMap<Local, Vec<Local>>,
}

/// Represents the kind of assignment for a local variable
#[derive(Debug, Clone)]
pub enum AssignmentKind<'tcx> {
    Const,
    FromPlace(Place<'tcx>),
    RefTo(Place<'tcx>),
    Other,
}

/// Represents the origin of a value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Constant,
    Parameter,
    Unknown,
}

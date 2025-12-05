use rustc_middle::mir::Local;
use rustc_middle::mir::Place;
use rustc_middle::ty::Ty;
use rustc_span::Span;
use std::collections::HashMap;

use rustc_middle::ty::{self as rustc_ty};

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

#[derive(Debug, Clone, PartialEq)]
pub enum NestedArgumentType {
    Ctx,
    Accounts,
    Account,
}

#[derive(Debug, Clone)]
pub struct NestedArgument<'tcx> {
    pub arg_type: NestedArgumentType,
    pub accounts: HashMap<String, NestedAccount<'tcx>>,
}

#[derive(Debug, Clone)]
pub struct NestedAccount<'tcx> {
    pub account_ty: Ty<'tcx>,
    pub account_local: Local,
}

#[derive(Debug, Clone)]
pub struct AccountNameAndLocal {
    pub account_name: String,
    pub account_local: Local,
}

/// Parameter information extracted from a function parameter
pub struct ParamData<'tcx> {
    pub param_index: usize,
    pub param_local: Local,
    pub param_name: String,
    pub param_ty: rustc_ty::Ty<'tcx>,
    pub adt_def: Option<(
        &'tcx rustc_ty::AdtDef<'tcx>,
        &'tcx rustc_ty::GenericArgsRef<'tcx>,
    )>,
    pub struct_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParamInfo<'tcx> {
    pub param_index: usize,
    pub param_name: String,
    pub param_local: Local,
    pub param_ty: Ty<'tcx>,
}

#[derive(Debug, Clone)]
pub struct UnsafeAccount {
    pub account_name: String,
    pub account_span: Span,
    pub is_mutable: bool,
    pub is_option: bool,
    #[allow(unused)]
    pub has_address_constraint: bool,
    pub constraints: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PdaSigner {
    pub account_name: String,
    pub account_span: Span,
    pub has_seeds: bool,
    pub seeds: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AccountConstraint {
    pub mutable: bool,
    pub has_address_constraint: bool,
    pub constraints: Vec<String>,
}

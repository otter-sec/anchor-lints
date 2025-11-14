use rustc_middle::mir::BasicBlock;
use rustc_middle::{mir::Local, ty::Ty};
use rustc_span::Span;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AnchorContextInfo<'tcx> {
    pub anchor_context_name: String,
    pub anchor_context_type: Ty<'tcx>,
    pub anchor_context_account_type: Ty<'tcx>,
    pub anchor_context_arg_accounts_type: HashMap<String, Ty<'tcx>>,
    #[allow(unused)]
    pub anchor_context_arg_local: Local,
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
    pub accounts: HashMap<String, (Ty<'tcx>, Local)>,
}

#[derive(Debug, Clone)]
pub struct AccountNameAndLocal {
    pub account_name: String,
    pub account_local: Local,
}

#[derive(Debug, Clone)]
pub struct NestedFunctionOperations<'tcx> {
    pub cpi_context_creation: HashMap<String, BasicBlock>,
    pub cpi_calls: HashMap<BasicBlock, Span>,
    pub nested_function_blocks: Vec<NestedFunctionBlocks<'tcx>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NestedBlockType {
    Reload,
    Access,
}

#[derive(Debug, Clone)]
pub struct NestedFunctionBlocks<'tcx> {
    pub account_name: String,
    pub account_ty: Ty<'tcx>,
    pub account_local: Local,
    pub account_span: Span,
    pub account_block: BasicBlock,
    pub stale_data_access: bool,
    pub block_type: NestedBlockType,
}

#[derive(Debug, Clone)]
pub struct AccountAccess {
    pub access_block: BasicBlock,
    pub access_span: Span,
    pub stale_data_access: bool,
}

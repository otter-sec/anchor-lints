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

#[derive(Debug, Clone)]
pub struct NestedFunctionOperations<'tcx> {
    pub cpi_context_creation: Vec<CpiContextCreationBlock>,
    pub cpi_calls: Vec<CpiCallBlock>,
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
    pub not_used_reload: bool,
}

#[derive(Debug, Clone)]
pub struct AccountAccess {
    pub access_block: BasicBlock,
    pub access_span: Span,
    pub stale_data_access: bool,
}

#[derive(Debug, Clone)]
pub struct CpiCallBlock {
    pub cpi_call_block: BasicBlock,
    pub cpi_call_span: Span,
}

#[derive(Debug, Clone)]
pub struct CpiContextCreationBlock {
    pub cpi_context_block: BasicBlock,
    pub account_name: String,
    pub cpi_context_local: Local,
}

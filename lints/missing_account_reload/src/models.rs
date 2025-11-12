use std::collections::HashMap;

use rustc_middle::{mir::Local, ty::Ty};

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

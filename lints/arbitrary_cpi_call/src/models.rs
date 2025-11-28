use rustc_middle::mir::{BasicBlock, Local};
use rustc_span::Span;

#[derive(Debug)]
pub struct CpiCallsInfo {
    pub span: Span,
    pub local: Local,
}

#[derive(Debug)]
pub struct CpiContextsInfo {
    pub cpi_ctx_local: Local,
    pub program_id_local: Local,
}

/// A switch on `discr`, where a truthy value leads to `then`
#[derive(Debug, Clone, Copy)]
pub struct IfThen {
    pub discr: Local,
    pub then: BasicBlock,
    pub els: BasicBlock,
}

#[derive(Debug, Clone, Copy)]
pub struct Cmp {
    pub lhs: Local,
    pub rhs: Local,
    pub ret: Local,
    pub is_eq: bool,
}

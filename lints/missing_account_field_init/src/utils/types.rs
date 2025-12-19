use rustc_middle::ty::Ty;
use rustc_span::Span;

/// To store initialized account information
#[derive(Debug, Clone)]
pub struct InitAccountInfo<'tcx> {
    pub inner_ty: Ty<'tcx>,
    pub span: Span,
    pub is_account_loader: bool,
}

/// To store account field information
#[derive(Debug, Clone)]
pub struct AccountField<'tcx> {
    pub name: String,
    #[allow(dead_code)]
    pub ty: Ty<'tcx>,
}

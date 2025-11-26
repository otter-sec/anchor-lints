use rustc_hir::def_id::DefId;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::Symbol;

#[derive(Copy, Clone, Debug)]
pub enum DiagnoticItem {
    /// `anchor_lang::accounts::account::Account::reload`
    AnchorAccountReload,
    /// `anchor_lang::context::CpiContext`
    AnchorCpiContext,
    AnchorCpiInvoke,
    AnchorCpiInvokeUnchecked,
    AnchorCpiInvokeSigned,
    AnchorCpiInvokeSignedUnchecked,
}

impl DiagnoticItem {
    fn diagnostic_item_name(&self) -> &'static str {
        match self {
            DiagnoticItem::AnchorAccountReload => "AnchorAccountReload",
            DiagnoticItem::AnchorCpiContext => "AnchorCpiContext",
            DiagnoticItem::AnchorCpiInvoke => "AnchorCpiInvoke",
            DiagnoticItem::AnchorCpiInvokeUnchecked => "AnchorCpiInvokeUnchecked",
            DiagnoticItem::AnchorCpiInvokeSigned => "AnchorCpiInvokeSigned",
            DiagnoticItem::AnchorCpiInvokeSignedUnchecked => "AnchorCpiInvokeSignedUnchecked",
        }
    }

    /// List of paths that this item may exist at
    fn paths(&self) -> &'static [&'static str] {
        match self {
            DiagnoticItem::AnchorAccountReload => {
                &["anchor_lang::accounts::account::Account::reload"]
            }
            DiagnoticItem::AnchorCpiContext => &["anchor_lang::context::CpiContext"],
            DiagnoticItem::AnchorCpiInvoke => &[
                "anchor_lang::solana_program::program::invoke",
                "solana_invoke::invoke",
            ],
            DiagnoticItem::AnchorCpiInvokeUnchecked => &[
                "anchor_lang::solana_program::program::invoke_unchecked",
                "solana_invoke::invoke_unchecked",
            ],
            DiagnoticItem::AnchorCpiInvokeSigned => &[
                "anchor_lang::solana_program::program::invoke_signed",
                "solana_invoke::invoke_signed",
            ],
            DiagnoticItem::AnchorCpiInvokeSignedUnchecked => &[
                "anchor_lang::solana_program::program::invoke_signed_unchecked",
                "solana_invoke::invoke_signed_unchecked",
            ],
        }
    }

    /// Check if a given [`DefId`] is a given diagnostic item. It will fall back to parsing item paths
    /// if the diagnostic item is not available.
    pub fn defid_is_item(&self, tcx: TyCtxt, def_id: DefId) -> bool {
        if let Some(diag_item) = tcx.get_diagnostic_name(def_id) {
            diag_item == Symbol::intern(self.diagnostic_item_name())
        } else {
            self.paths().contains(&tcx.def_path_str(def_id).as_str())
        }
    }

    /// Check if a given [`Ty`] is a given diagnostic item. It will fall back to parsing type paths
    /// if the diagnostic item is not available.
    pub fn defid_is_type(&self, tcx: TyCtxt, ty: Ty) -> bool {
        let ty::Adt(adt, _) = ty.kind() else {
            return false;
        };
        let type_def_id = adt.did();
        let map = &tcx.diagnostic_items(type_def_id.krate).name_to_id;
        if let Some(&diag_item) = map.get(&Symbol::intern(self.diagnostic_item_name())) {
            return diag_item == type_def_id;
        } else {
            self.paths()
                .contains(&tcx.def_path_str(type_def_id).as_str())
        }
    }
}

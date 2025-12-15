use rustc_hir::def_id::DefId;
use rustc_lint::LateContext;

use once_cell::sync::Lazy;
use std::collections::HashMap;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum CpiKind {
    SetAuthority,
    SetAuthorityStruct,
    Burn,
    BurnStruct,
    MintTo,
    MintToStruct,
    CreateAta,
    CreateAtaStruct,
    Transfer,
    TransferStruct,
    SystemTransfer,
    SystemTransferStruct,
    Token2022Transfer,
    Token2022TransferChecked,
    CloseAccount,
    CloseAccountStruct,
    FreezeAccount,
    FreezeAccountStruct,
    ThawAccount,
    ThawAccountStruct,
    Approve,
    ApproveStruct,
    Revoke,
    RevokeStruct,
    SyncNative,
    SyncNativeStruct,
    Token2022MintToChecked,
    Token2022BurnChecked,
}

pub static CPI_PATHS: Lazy<HashMap<CpiKind, Vec<&'static str>>> = Lazy::new(|| {
    use CpiKind::*;
    HashMap::from([
        (SetAuthority, vec!["anchor_spl::token::set_authority"]),
        (SetAuthorityStruct, vec!["anchor_spl::token::SetAuthority"]),
        (Burn, vec!["anchor_spl::token::burn"]),
        (BurnStruct, vec!["anchor_spl::token::Burn"]),
        (MintTo, vec!["anchor_spl::token::mint_to"]),
        (MintToStruct, vec!["anchor_spl::token::MintTo"]),
        (CreateAta, vec!["anchor_spl::associated_token::create"]),
        (
            CreateAtaStruct,
            vec!["anchor_spl::associated_token::Create"],
        ),
        (Transfer, vec!["anchor_spl::token::transfer"]),
        (TransferStruct, vec!["anchor_spl::token::Transfer"]),
        (
            SystemTransfer,
            vec!["anchor_lang::system_program::transfer"],
        ),
        (
            SystemTransferStruct,
            vec!["anchor_lang::system_program::Transfer"],
        ),
        (
            Token2022Transfer,
            vec!["anchor_spl::token_2022::spl_token_2022::instruction::transfer"],
        ),
        (
            Token2022TransferChecked,
            vec!["anchor_spl::token_2022::spl_token_2022::instruction::transfer_checked"],
        ),
        (CloseAccount, vec!["anchor_spl::token::close_account"]),
        (CloseAccountStruct, vec!["anchor_spl::token::CloseAccount"]),
        (FreezeAccount, vec!["anchor_spl::token::freeze_account"]),
        (FreezeAccountStruct, vec!["anchor_spl::token::FreezeAccount"]),
        (ThawAccount, vec!["anchor_spl::token::thaw_account"]),
        (ThawAccountStruct, vec!["anchor_spl::token::ThawAccount"]),
        (Approve, vec!["anchor_spl::token::approve"]),
        (ApproveStruct, vec!["anchor_spl::token::Approve"]),
        (Revoke, vec!["anchor_spl::token::revoke"]),
        (RevokeStruct, vec!["anchor_spl::token::Revoke"]),
        (SyncNative, vec!["anchor_spl::token::sync_native"]),
        (SyncNativeStruct, vec!["anchor_spl::token::SyncNative"]),
        (
            Token2022MintToChecked,
            vec!["anchor_spl::token_2022::spl_token_2022::instruction::mint_to_checked"],
        ),
        (
            Token2022BurnChecked,
            vec!["anchor_spl::token_2022::spl_token_2022::instruction::burn_checked"],
        ),
    ])
});


pub fn matches_cpi_kind<'tcx>(
    cx: &LateContext<'tcx>,
    def_id: DefId,
    kind: CpiKind,
) -> bool {
    let path = cx.tcx.def_path_str(def_id);

    if let Some(paths) = CPI_PATHS.get(&kind) {
        return paths.iter().any(|p| *p == path);
    }
    false
}


pub fn detect_cpi_kind<'tcx>(
    cx: &LateContext<'tcx>,
    def_id: DefId,
) -> Option<CpiKind> {
    let path = cx.tcx.def_path_str(def_id);

    for (kind, list) in CPI_PATHS.iter() {
        if list.iter().any(|p| *p == path) {
            return Some(*kind);
        }
    }

    None
}

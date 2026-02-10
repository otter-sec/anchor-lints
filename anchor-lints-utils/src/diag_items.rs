#![allow(clippy::disallowed_methods)] // We use `def_path_str` as a fallback

use rustc_hir::def_id::DefId;
use rustc_middle::ty::{self, Ty, TyCtxt, TyKind};
use rustc_span::Symbol;

#[derive(Copy, Clone, Debug)]
pub enum DiagnoticItem {
    /// `anchor_lang::accounts::account::Account`
    AnchorAccount,
    /// `anchor_lang::prelude::AccountLoader`
    AnchorAccountLoader,
    /// `anchor_lang::accounts::account::Account::reload`
    AnchorAccountReload,
    /// `anchor_lang::context::CpiContext`
    AnchorCpiContext,
    /// `anchor_lang::context::CpiContext::with_remaining_accounts`
    AnchorCpiContextWithRemainingAccounts,
    /// `anchor_lang::context::Context`
    AnchorContext,
    AnchorCpiInvoke,
    AnchorCpiInvokeUnchecked,
    AnchorCpiInvokeSigned,
    AnchorCpiInvokeSignedUnchecked,
    /// `anchor_lang::prelude::InterfaceAccount`
    AnchorInterfaceAccount,
    /// `anchor_lang::prelude::Key::key`
    AnchorKey,
    /// `anchor_lang::prelude::Account::set_inner`
    AnchorAccountSetInner,
    /// `anchor_lang::prelude::Signer`
    AnchorSigner,
    /// `anchor_lang::system_program::transfer`
    AnchorSystemProgramTransfer,
    /// `anchor_lang::system_program::assign`
    AnchorSystemProgramAssign,
    /// `anchor_lang::system_program::allocate`
    AnchorSystemProgramAllocate,
    /// `anchor_lang::system_program::create_account`
    AnchorSystemProgramCreateAccount,
    /// `anchor_lang::prelude::SystemAccount`
    AnchorSystemAccount,
    /// `anchor_lang::ToAccountInfo::to_account_info`
    AnchorToAccountInfo,
    /// `anchor_lang::prelude::UncheckedAccount`
    AnchorUncheckedAccount,
    /// `anchor_spl::token::transfer`
    AnchorSplTokenTransfer,
    /// `anchor_spl::token::TokenAccount`
    AnchorSplTokenAccount,
    /// `anchor_spl::token_interface::TokenAccount`
    AnchorSplTokenInterfaceTokenAccount,
    /// `anchor_spl::token::Mint`
    AnchorSplTokenMint,
    /// `anchor_spl::token_interface::Mint`
    AnchorSplTokenInterfaceMint,
    /// `anchor_spl::token_interface::get_account_data_size`
    AnchorSplTokenInterfaceGetAccountDataSize,
    /// `anchor_spl::token_interface::get_extension_data`
    AnchorSplTokenInterfaceGetExtensionData,
    /// `anchor_spl::token_interface::get_account_len`
    AnchorSplTokenInterfaceGetAccountLen,
    /// `anchor_spl::token_interface::get_mint_len`
    AnchorSplTokenInterfaceGetMintLen,
    /// `anchor_spl::token_2022::get_account_data_size`
    AnchorSplToken2022GetAccountDataSize,
    /// `pyth_solana_receiver_sdk::price_update::PriceUpdateV2`
    PythPriceUpdateV2,
    /// `pyth_solana_receiver_sdk::price_update::PriceUpdateV2::get_price_no_older_than`
    PythPriceUpdateV2GetPriceNoOlderThan,
    /// `solana_program::account_info::AccountInfo`
    SolanaAccountInfo,
    /// `solana_program::instruction::Instruction`
    SolanaInstruction,
    /// `solana_program::pubkey::Pubkey`
    SolanaPubkey,
    /// `spl_token::state::Account`
    SplTokenAccount,
    /// `spl_token::state::Mint`
    SplTokenMint,
}

impl DiagnoticItem {
    /// Returns the name of the defined DiagnosticItem, if one exists
    /// There are no diagnostic items for upstream types (e.g. Solana or Pyth)
    fn diagnostic_item_name(&self) -> Option<&'static str> {
        Some(match self {
            DiagnoticItem::AnchorAccount => "AnchorAccount",
            DiagnoticItem::AnchorAccountLoader => "AnchorAccountLoader",
            DiagnoticItem::AnchorAccountReload => "AnchorAccountReload",
            DiagnoticItem::AnchorCpiContext => "AnchorCpiContext",
            DiagnoticItem::AnchorCpiContextWithRemainingAccounts => {
                "AnchorCpiContextWithRemainingAccounts"
            }
            DiagnoticItem::AnchorContext => "AnchorContext",
            DiagnoticItem::AnchorCpiInvoke => "AnchorCpiInvoke",
            DiagnoticItem::AnchorCpiInvokeUnchecked => "AnchorCpiInvokeUnchecked",
            DiagnoticItem::AnchorCpiInvokeSigned => "AnchorCpiInvokeSigned",
            DiagnoticItem::AnchorCpiInvokeSignedUnchecked => "AnchorCpiInvokeSignedUnchecked",
            DiagnoticItem::AnchorInterfaceAccount => "AnchorInterfaceAccount",
            DiagnoticItem::AnchorKey => "AnchorKey",
            DiagnoticItem::AnchorAccountSetInner => "AnchorAccountSetInner",
            DiagnoticItem::AnchorSigner => "AnchorSigner",
            DiagnoticItem::AnchorSystemProgramTransfer => "AnchorSystemProgramTransfer",
            DiagnoticItem::AnchorSystemProgramAssign => "AnchorSystemProgramAssign",
            DiagnoticItem::AnchorSystemProgramAllocate => "AnchorSystemProgramAllocate",
            DiagnoticItem::AnchorSystemProgramCreateAccount => "AnchorSystemProgramCreateAccount",
            DiagnoticItem::AnchorSystemAccount => "AnchorSystemAccount",
            DiagnoticItem::AnchorToAccountInfo => "AnchorToAccountInfo",
            DiagnoticItem::AnchorUncheckedAccount => "AnchorUncheckedAccount",
            DiagnoticItem::AnchorSplTokenTransfer => "AnchorSplTokenTransfer",
            DiagnoticItem::AnchorSplTokenAccount => "AnchorSplTokenAccount",
            DiagnoticItem::AnchorSplTokenInterfaceTokenAccount => {
                "AnchorSplTokenInterfaceTokenAccount"
            }
            DiagnoticItem::AnchorSplTokenMint => "AnchorSplTokenMint",
            DiagnoticItem::AnchorSplTokenInterfaceMint => "AnchorSplTokenInterfaceMint",
            DiagnoticItem::AnchorSplTokenInterfaceGetAccountDataSize => {
                "AnchorSplTokenInterfaceGetAccountDataSize"
            }
            DiagnoticItem::AnchorSplTokenInterfaceGetExtensionData => {
                "AnchorSplTokenInterfaceGetExtensionData"
            }
            DiagnoticItem::AnchorSplTokenInterfaceGetAccountLen => {
                "AnchorSplTokenInterfaceGetAccountLen"
            }
            DiagnoticItem::AnchorSplTokenInterfaceGetMintLen => "AnchorSplTokenInterfaceGetMintLen",
            DiagnoticItem::AnchorSplToken2022GetAccountDataSize => {
                "AnchorSplToken2022GetAccountDataSize"
            }
            DiagnoticItem::PythPriceUpdateV2 => {
                return None;
            }
            DiagnoticItem::PythPriceUpdateV2GetPriceNoOlderThan => {
                return None;
            }
            DiagnoticItem::SolanaAccountInfo => {
                return None;
            }
            DiagnoticItem::SolanaInstruction => {
                return None;
            }
            DiagnoticItem::SolanaPubkey => {
                return None;
            }
            DiagnoticItem::SplTokenAccount => {
                return None;
            }
            DiagnoticItem::SplTokenMint => {
                return None;
            }
        })
    }

    /// List of paths that this item may exist at
    fn paths(&self) -> &'static [&'static str] {
        match self {
            DiagnoticItem::AnchorAccount => &[
                "anchor_lang::accounts::account::Account",
                "anchor_lang::prelude::Account",
            ],
            DiagnoticItem::AnchorAccountLoader => &["anchor_lang::prelude::AccountLoader"],
            DiagnoticItem::AnchorAccountReload => {
                &["anchor_lang::accounts::account::Account::reload"]
            }
            DiagnoticItem::AnchorCpiContext => &["anchor_lang::context::CpiContext"],
            DiagnoticItem::AnchorCpiContextWithRemainingAccounts => &[
                "anchor_lang::context::CpiContext::with_remaining_accounts",
                "anchor_lang::prelude::CpiContext::with_remaining_accounts",
            ],
            DiagnoticItem::AnchorContext => &[
                "anchor_lang::context::Context",
                "anchor_lang::prelude::Context",
            ],
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
            DiagnoticItem::AnchorInterfaceAccount => &[
                "anchor_lang::prelude::InterfaceAccount",
                "anchor_lang::accounts::interface_account::InterfaceAccount",
                "anchor_spl::token::InterfaceAccount",
            ],
            DiagnoticItem::AnchorKey => {
                &["anchor_lang::prelude::Key::key", "anchor_lang::Key::key"]
            }
            DiagnoticItem::AnchorAccountSetInner => &[
                "anchor_lang::prelude::Account::set_inner",
                "anchor_lang::accounts::account::Account::set_inner",
            ],
            DiagnoticItem::AnchorSigner => &["anchor_lang::prelude::Signer"],
            DiagnoticItem::AnchorSystemProgramTransfer => {
                &["anchor_lang::system_program::transfer"]
            }
            DiagnoticItem::AnchorSystemProgramAssign => &["anchor_lang::system_program::assign"],
            DiagnoticItem::AnchorSystemProgramAllocate => {
                &["anchor_lang::system_program::allocate"]
            }
            DiagnoticItem::AnchorSystemProgramCreateAccount => {
                &["anchor_lang::system_program::create_account"]
            }
            DiagnoticItem::AnchorSystemAccount => &["anchor_lang::prelude::SystemAccount"],
            DiagnoticItem::AnchorToAccountInfo => &[
                "anchor_lang::ToAccountInfo::to_account_info",
                "anchor_lang::prelude::ToAccountInfo::to_account_info",
            ],
            DiagnoticItem::AnchorUncheckedAccount => &["anchor_lang::prelude::UncheckedAccount"],
            DiagnoticItem::AnchorSplTokenTransfer => &["anchor_spl::token::transfer"],
            DiagnoticItem::AnchorSplTokenAccount => &["anchor_spl::token::TokenAccount"],
            DiagnoticItem::AnchorSplTokenInterfaceTokenAccount => {
                &["anchor_spl::token_interface::TokenAccount"]
            }
            DiagnoticItem::AnchorSplTokenMint => &["anchor_spl::token::Mint"],
            DiagnoticItem::AnchorSplTokenInterfaceMint => &["anchor_spl::token_interface::Mint"],
            DiagnoticItem::AnchorSplTokenInterfaceGetAccountDataSize => {
                &["anchor_spl::token_interface::get_account_data_size"]
            }
            DiagnoticItem::AnchorSplTokenInterfaceGetExtensionData => {
                &["anchor_spl::token_interface::get_extension_data"]
            }
            DiagnoticItem::AnchorSplTokenInterfaceGetAccountLen => {
                &["anchor_spl::token_interface::get_account_len"]
            }
            DiagnoticItem::AnchorSplTokenInterfaceGetMintLen => {
                &["anchor_spl::token_interface::get_mint_len"]
            }
            DiagnoticItem::AnchorSplToken2022GetAccountDataSize => {
                &["anchor_spl::token_2022::get_account_data_size"]
            }
            DiagnoticItem::PythPriceUpdateV2 => {
                &["pyth_solana_receiver_sdk::price_update::PriceUpdateV2"]
            }
            DiagnoticItem::PythPriceUpdateV2GetPriceNoOlderThan => {
                &["pyth_solana_receiver_sdk::price_update::PriceUpdateV2::get_price_no_older_than"]
            }
            DiagnoticItem::SolanaAccountInfo => &["solana_program::account_info::AccountInfo"],
            DiagnoticItem::SolanaInstruction => &[
                "solana_program::instruction::Instruction",
                "solana_program::instruction::CompiledInstruction",
            ],
            DiagnoticItem::SolanaPubkey => {
                &["solana_program::pubkey::Pubkey", "solana_pubkey::Pubkey"]
            }
            DiagnoticItem::SplTokenAccount => &["spl_token::state::Account"],
            DiagnoticItem::SplTokenMint => &["spl_token::state::Mint"],
        }
    }

    /// Check if a given [`DefId`] is a given diagnostic item. It will fall back to parsing item paths
    /// if the diagnostic item is not available.
    pub fn defid_is_item(&self, tcx: TyCtxt, def_id: DefId) -> bool {
        if let Some(diag_item) = tcx.get_diagnostic_name(def_id)
            && let Some(diag_item_name) = self.diagnostic_item_name()
        {
            diag_item == Symbol::intern(diag_item_name)
        } else {
            let path = tcx.def_path_str(def_id);
            let path = path.as_str();
            self.paths().iter().any(|p| path.ends_with(p))
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
        if let Some(diag_item_name) = self.diagnostic_item_name()
            && let Some(&diag_item) = map.get(&Symbol::intern(diag_item_name))
        {
            diag_item == type_def_id
        } else {
            let path = tcx.def_path_str(type_def_id);
            let path = path.as_str();
            self.paths().iter().any(|p| path.ends_with(p))
        }
    }
}

/// Check if a given [`DefId`] is a CPI invoke function.
pub fn is_cpi_invoke_fn(tcx: TyCtxt, def_id: DefId) -> bool {
    [
        DiagnoticItem::AnchorCpiInvoke,
        DiagnoticItem::AnchorCpiInvokeUnchecked,
        DiagnoticItem::AnchorCpiInvokeSigned,
        DiagnoticItem::AnchorCpiInvokeSignedUnchecked,
    ]
    .iter()
    .any(|item| item.defid_is_item(tcx, def_id))
}

/// Check if a given [`Ty`] is a CPI context.
pub fn is_anchor_cpi_context(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::AnchorCpiContext.defid_is_type(tcx, ty)
}

pub fn is_anchor_context(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::AnchorContext.defid_is_type(tcx, ty)
}

pub fn is_anchor_account_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::AnchorAccount.defid_is_type(tcx, ty)
}

/// If this type is an [`DiagnosticItem::AnchorAccount`], get the inner type
pub fn anchor_inner_account_type<'tcx>(tcx: TyCtxt, ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
    if is_anchor_account_type(tcx, ty) {
        let TyKind::Adt(_, substs) = ty.peel_refs().kind() else {
            unreachable!()
        };
        substs.types().next()
    } else {
        None
    }
}

pub fn is_anchor_account_loader_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::AnchorAccountLoader.defid_is_type(tcx, ty)
}

pub fn is_anchor_interface_account_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::AnchorInterfaceAccount.defid_is_type(tcx, ty)
}

pub fn is_anchor_system_account_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::AnchorSystemAccount.defid_is_type(tcx, ty)
}

pub fn is_anchor_signer_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::AnchorSigner.defid_is_type(tcx, ty)
}

pub fn is_anchor_unchecked_account_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::AnchorUncheckedAccount.defid_is_type(tcx, ty)
}

pub fn is_account_info_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::SolanaAccountInfo.defid_is_type(tcx, ty)
}

pub fn is_solana_pubkey_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::SolanaPubkey.defid_is_type(tcx, ty)
}

pub fn is_anchor_account_set_inner_fn(tcx: TyCtxt, def_id: DefId) -> bool {
    DiagnoticItem::AnchorAccountSetInner.defid_is_item(tcx, def_id)
}

pub fn is_anchor_to_account_info_fn(tcx: TyCtxt, def_id: DefId) -> bool {
    DiagnoticItem::AnchorToAccountInfo.defid_is_item(tcx, def_id)
}

pub fn is_anchor_cpi_context_with_remaining_accounts_fn(tcx: TyCtxt, def_id: DefId) -> bool {
    DiagnoticItem::AnchorCpiContextWithRemainingAccounts.defid_is_item(tcx, def_id)
}

pub fn is_anchor_key_fn(tcx: TyCtxt, def_id: DefId) -> bool {
    DiagnoticItem::AnchorKey.defid_is_item(tcx, def_id)
}

pub fn is_anchor_system_program_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty::Adt(adt, _) = ty.kind() else {
        return false;
    };
    let def_id = adt.did();
    // FIXME: Use diag items
    tcx.def_path_str(def_id)
        .as_str()
        .contains("::system_program::")
}

pub fn is_anchor_system_program_lamports_only_cpi(tcx: TyCtxt, def_id: DefId) -> bool {
    [
        DiagnoticItem::AnchorSystemProgramTransfer,
        DiagnoticItem::AnchorSystemProgramAssign,
        DiagnoticItem::AnchorSystemProgramAllocate,
        DiagnoticItem::AnchorSystemProgramCreateAccount,
    ]
    .iter()
    .any(|item| item.defid_is_item(tcx, def_id))
}

pub fn is_anchor_spl_token_interface_safe_cpi(tcx: TyCtxt, def_id: DefId) -> bool {
    [
        DiagnoticItem::AnchorSplToken2022GetAccountDataSize,
        DiagnoticItem::AnchorSplTokenInterfaceGetAccountDataSize,
        DiagnoticItem::AnchorSplTokenInterfaceGetExtensionData,
        DiagnoticItem::AnchorSplTokenInterfaceGetAccountLen,
        DiagnoticItem::AnchorSplTokenInterfaceGetMintLen,
    ]
    .iter()
    .any(|item| item.defid_is_item(tcx, def_id))
}

pub fn is_anchor_spl_token_account_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::AnchorSplTokenAccount.defid_is_type(tcx, ty)
}

pub fn is_anchor_spl_token_interface_token_account_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::AnchorSplTokenInterfaceTokenAccount.defid_is_type(tcx, ty)
}

pub fn is_spl_token_account_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::SplTokenAccount.defid_is_type(tcx, ty)
}

pub fn is_anchor_spl_token_mint_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::AnchorSplTokenMint.defid_is_type(tcx, ty)
}

pub fn is_anchor_spl_token_interface_mint_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::AnchorSplTokenInterfaceMint.defid_is_type(tcx, ty)
}

pub fn is_spl_token_mint_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::SplTokenMint.defid_is_type(tcx, ty)
}

pub fn is_pyth_price_update_v2_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::PythPriceUpdateV2.defid_is_type(tcx, ty)
}

pub fn is_pyth_get_price_no_older_than_fn(tcx: TyCtxt, def_id: DefId) -> bool {
    DiagnoticItem::PythPriceUpdateV2GetPriceNoOlderThan.defid_is_item(tcx, def_id)
}

pub fn is_solana_instruction_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    DiagnoticItem::SolanaInstruction.defid_is_type(tcx, ty)
}

pub fn is_box_type(tcx: TyCtxt, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    if let ty::Adt(adt_def, _) = ty.kind() {
        if let Some(box_def_id) = tcx.lang_items().owned_box() {
            return adt_def.did() == box_def_id;
        }
    }
    false
}

pub fn is_constructor_like_fn(tcx: TyCtxt, def_id: DefId) -> bool {
    tcx.item_name(def_id).as_str().starts_with("new")
}

pub fn is_borrow_fn(tcx: TyCtxt, def_id: DefId) -> bool {
    matches!(tcx.item_name(def_id).as_str(), "borrow" | "borrow_mut")
}

pub fn is_deserialize_fn(tcx: TyCtxt, def_id: DefId) -> bool {
    tcx.item_name(def_id).as_str().contains("deserialize")
}

pub fn is_cpi_builder_constructor_fn(tcx: TyCtxt, def_id: DefId) -> bool {
    let path = tcx.def_path_str(def_id);
    path.contains("CpiBuilder::new")
        || path.contains("CpiBuilder")
        || path.contains("DelegateStaking")
        || path.contains("LockV1")
        || path.contains("UnlockV1")
        || path.contains("RevokeStaking")
}

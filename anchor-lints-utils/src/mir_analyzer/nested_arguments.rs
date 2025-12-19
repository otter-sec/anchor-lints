use clippy_utils::source::HasSession;
use rustc_middle::{
    mir::{HasLocalDecls, Local},
    ty::{self as rustc_ty},
};
use rustc_span::source_map::Spanned;

use std::collections::HashMap;

use super::types::{AnchorContextInfo, MirAnalyzer};
use crate::models::*;
use crate::utils::{extract_account_name_from_string, remove_comments};

impl<'cx, 'tcx> MirAnalyzer<'cx, 'tcx> {
    // Helper to extract (local, account_ty) from an operand
    pub fn extract_local_and_ty_from_operand(
        &self,
        arg: &Spanned<rustc_middle::mir::Operand<'tcx>>,
    ) -> Option<(Local, rustc_ty::Ty<'tcx>)> {
        if let rustc_middle::mir::Operand::Move(place) | rustc_middle::mir::Operand::Copy(place) =
            &arg.node
            && let Some(local) = place.as_local()
            && let Some(account_ty) = self.mir.local_decls().get(local).map(|d| d.ty.peel_refs())
        {
            Some((local, account_ty))
        } else {
            None
        }
    }

    // Helper to create NestedAccount from account_ty and arg_index
    fn create_nested_account(
        account_ty: rustc_ty::Ty<'tcx>,
        arg_index: usize,
    ) -> NestedAccount<'tcx> {
        NestedAccount {
            account_ty,
            account_local: Local::from_usize(arg_index + 1),
        }
    }

    // Helper to extract account name from span, with fallback
    fn extract_account_name_from_span(
        &self,
        arg: &Spanned<rustc_middle::mir::Operand<'tcx>>,
        fallback_name: Option<&String>,
    ) -> Option<String> {
        if let Ok(snippet) = self.cx.sess().source_map().span_to_snippet(arg.span) {
            let cleaned_snippet = remove_comments(&snippet);
            if let Some(acc_name) = extract_account_name_from_string(&cleaned_snippet) {
                return Some(acc_name);
            }
        }
        fallback_name.cloned()
    }

    // Extracts arguments if they contain context/context.accounts/context.accounts.account
    pub fn get_nested_fn_arguments(
        &self,
        args: &[Spanned<rustc_middle::mir::Operand<'tcx>>],
        anchor_context_info: Option<&AnchorContextInfo<'tcx>>,
    ) -> Option<NestedArgument<'tcx>> {
        let mut nested_argument = NestedArgument {
            arg_type: NestedArgumentType::Ctx,
            accounts: HashMap::new(),
        };
        let mut found = false;
        let cpi_context_info = anchor_context_info.or(self.anchor_context_info.as_ref());

        for (arg_index, arg) in args.iter().enumerate() {
            let Some((_local, account_ty)) = self.extract_local_and_ty_from_operand(arg) else {
                continue;
            };

            let Some(cpi_context_info) = cpi_context_info else {
                continue;
            };

            if account_ty == cpi_context_info.anchor_context_type {
                nested_argument.arg_type = NestedArgumentType::Ctx;
                found = true;
                break;
            } else if account_ty == cpi_context_info.anchor_context_account_type {
                nested_argument.arg_type = NestedArgumentType::Accounts;
                found = true;
                break;
            } else if let Some((account_name, _)) = cpi_context_info
                .anchor_context_arg_accounts_type
                .iter()
                .find(|(_, accty)| *accty == &account_ty || self.is_account_info_type(account_ty))
            {
                if let Some(acc_name) = self.extract_account_name_from_span(arg, Some(account_name))
                {
                    nested_argument
                        .accounts
                        .insert(acc_name, Self::create_nested_account(account_ty, arg_index));
                }
                nested_argument.arg_type = NestedArgumentType::Account;
                found = true;
            }
        }

        if found { Some(nested_argument) } else { None }
    }

    // Extracts arguments if they contain context/context.accounts/context.accounts.account as arguments
    pub fn get_nested_fn_arguments_as_params(
        &self,
        args: &[Spanned<rustc_middle::mir::Operand<'tcx>>],
    ) -> Option<NestedArgument<'tcx>> {
        let mut nested_argument = NestedArgument {
            arg_type: NestedArgumentType::Account,
            accounts: HashMap::new(),
        };
        let mut found = false;

        for (arg_index, arg) in args.iter().enumerate() {
            let Some((local, account_ty)) = self.extract_local_and_ty_from_operand(arg) else {
                continue;
            };

            if self.is_account_info_type(account_ty)
                && let Some(param) = self.check_local_is_param(local)
            {
                nested_argument.accounts.insert(
                    param.param_name.clone(),
                    Self::create_nested_account(account_ty, arg_index),
                );
                found = true;
            }
        }

        if found { Some(nested_argument) } else { None }
    }
}

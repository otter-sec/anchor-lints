use clippy_utils::source::HasSession;
use rustc_lint::LateContext;
use rustc_middle::{mir::Local, ty::TyKind};

use std::collections::HashSet;

use super::types::MirAnalyzer;
use crate::models::*;
use crate::utils::{
    extract_account_constraints, is_option_unchecked_account_type, is_pda_account,
    is_unchecked_account_type, remove_comments,
};

impl<'cx, 'tcx> MirAnalyzer<'cx, 'tcx> {
    pub fn check_local_is_param(&self, local: Local) -> Option<&ParamInfo<'tcx>> {
        let local = self.resolve_to_original_local(local, &mut HashSet::new());
        for param in &self.param_info {
            if param.param_local == local {
                return Some(param);
            }
        }

        if let Some(span) = self.get_span_from_local(&local)
            && let Ok(snippet) = self.cx.sess().source_map().span_to_snippet(span)
        {
            let cleaned_snippet = remove_comments(&snippet);

            // Extract account name from patterns like "program.key()", "account.field", etc.
            let account_name = cleaned_snippet
                .split('.')
                .next()
                .map(|s| {
                    s.trim()
                        .trim_start_matches("&mut ")
                        .trim_start_matches("& ")
                })
                .filter(|s| !s.is_empty());

            if let Some(account_name) = account_name {
                // Check if any parameter matches this account name
                for param in &self.param_info {
                    if param.param_name == account_name {
                        return Some(param);
                    }
                }
            }
        }
        None
    }

    pub fn extract_unsafe_accounts_and_pdas(&self) -> (Vec<UnsafeAccount>, Vec<PdaSigner>) {
        let mut unsafe_accounts = Vec::new();
        let mut pda_signers = Vec::new();
        if let Some(anchor_context_info) = &self.anchor_context_info {
            let accounts_struct_ty = &anchor_context_info.anchor_context_account_type;

            if let TyKind::Adt(accounts_adt_def, accounts_generics) = accounts_struct_ty.kind() {
                if !accounts_adt_def.is_struct() && !accounts_adt_def.is_union() {
                    return (unsafe_accounts, pda_signers);
                }
                let variant = accounts_adt_def.non_enum_variant();
                for account_field in &variant.fields {
                    let account_name = account_field.ident(self.cx.tcx).to_string();
                    let account_ty = account_field.ty(self.cx.tcx, accounts_generics);
                    let account_span = self.cx.tcx.def_span(account_field.did);

                    let cx_ref: &LateContext<'tcx> = self.cx;
                    let is_option = is_option_unchecked_account_type(cx_ref, account_ty);
                    let is_unsafe = is_unchecked_account_type(cx_ref, account_ty) || is_option;

                    if is_unsafe {
                        let constraints = extract_account_constraints(cx_ref, account_field);

                        if constraints.mutable {
                            unsafe_accounts.push(UnsafeAccount {
                                account_name,
                                account_span,
                                is_mutable: constraints.mutable,
                                is_option,
                                has_address_constraint: constraints.has_address_constraint,
                                constraints: constraints.constraints,
                            });
                        }
                    }

                    if let Some(pda_signer) = is_pda_account(cx_ref, account_field) {
                        pda_signers.push(pda_signer);
                    }
                }
            }
        }

        (unsafe_accounts, pda_signers)
    }
}

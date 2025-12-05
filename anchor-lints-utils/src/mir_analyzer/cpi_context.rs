use clippy_utils::source::HasSession;
use rustc_middle::{
    mir::{HasLocalDecls, Local},
    ty::{self as rustc_ty, TyKind},
};
use rustc_span::source_map::Spanned;

use std::collections::HashSet;

use super::types::{AnchorContextInfo, MirAnalyzer};
use crate::utils::remove_comments;
use crate::{diag_items::DiagnoticItem, models::*};

impl<'cx, 'tcx> MirAnalyzer<'cx, 'tcx> {
    pub fn is_from_cpi_context(
        &self,
        raw_local: Local,
        parent_anchor_context_info: Option<&AnchorContextInfo<'tcx>>,
    ) -> Option<CpiAccountInfo> {
        let get_anchor_context_info =
            parent_anchor_context_info.or(self.anchor_context_info.as_ref());
        if let Some(anchor_context_info) = &get_anchor_context_info {
            let local = self.resolve_to_original_local(raw_local, &mut HashSet::new());

            let local_decl = self.mir.local_decls().get(local)?;
            let local_ty = local_decl.ty.peel_refs();
            let span = local_decl.source_info.span;
            // Check if this is AccountInfo type (from .to_account_info() calls)
            // If so, we can't match by type - must use name-based matching
            let is_account_info_type = matches!(local_ty.kind(), TyKind::Adt(adt, _) if {
                let def_path = self.cx.tcx.def_path_str(adt.did());
                def_path == "solana_program::account_info::AccountInfo"
                    || def_path.ends_with("::AccountInfo")
            });

            let mut matching_accounts: Vec<(&String, &rustc_ty::Ty<'tcx>)> = if is_account_info_type
            {
                // For AccountInfo, we can't match by type - collect all accounts for name-based matching
                anchor_context_info
                    .anchor_context_arg_accounts_type
                    .iter()
                    .collect()
            } else {
                // Try to match by type first
                anchor_context_info
                    .anchor_context_arg_accounts_type
                    .iter()
                    .filter(|(_, account_ty)| {
                        let account_ty_peeled = account_ty.peel_refs();
                        match (local_ty.kind(), account_ty_peeled.kind()) {
                            (TyKind::Adt(local_adt, _), TyKind::Adt(account_adt, _)) => {
                                local_adt.did() == account_adt.did()
                            }
                            _ => local_ty == account_ty_peeled,
                        }
                    })
                    .collect()
            };

            if matching_accounts.len() == 1 {
                let (account_name, _) = matching_accounts[0];
                return Some(CpiAccountInfo {
                    account_name: account_name.clone(),
                    account_local: anchor_context_info.anchor_context_arg_local,
                });
            }

            if matching_accounts.is_empty() {
                matching_accounts = anchor_context_info
                    .anchor_context_arg_accounts_type
                    .iter()
                    .collect();
            }

            // Multiple matches — try to disambiguate using the span text (ctx.accounts.<name>)
            if let Ok(snippet) = self.cx.sess().source_map().span_to_snippet(span) {
                let cleaned_snippet = remove_comments(&snippet);

                // Helper function to find account by name
                let find_account_by_name = |name: &str| -> Option<String> {
                    matching_accounts
                        .iter()
                        .find(|(account_name, _)| account_name.as_str() == name)
                        .map(|(account_name, _)| (*account_name).clone())
                };

                // Try to extract account name from snippet patterns
                let account_name =
                    if let Some(after_accounts) = cleaned_snippet.split(".accounts.").nth(1) {
                        // Pattern: ctx.accounts.<name>
                        after_accounts
                            .split('.')
                            .next()
                            .and_then(|s| find_account_by_name(s.trim()))
                    } else if cleaned_snippet
                        .starts_with(anchor_context_info.anchor_context_name.as_str())
                    {
                        // Pattern: ctx.<name> (after removing ctx prefix)
                        let remaining = cleaned_snippet
                            .replace(anchor_context_info.anchor_context_name.as_str(), "");
                        remaining
                            .split('.')
                            .nth(1)
                            .and_then(|s| find_account_by_name(s.trim()))
                    } else if (cleaned_snippet.starts_with("self")
                        || cleaned_snippet.starts_with("&self"))
                        && parent_anchor_context_info.is_some()
                    {
                        let mut remaining = if cleaned_snippet.starts_with("&self") {
                            cleaned_snippet
                                .strip_prefix("&self")
                                .unwrap_or(&cleaned_snippet)
                        } else {
                            cleaned_snippet
                                .strip_prefix("self")
                                .unwrap_or(&cleaned_snippet)
                        }
                        .to_string();

                        // Remove leading whitespace, newlines, and dots
                        remaining = remaining
                            .trim_start_matches(|c: char| c.is_whitespace() || c == '.')
                            .to_string();
                        let remaining = remaining.trim();
                        let account_name = remaining
                            .split(|c: char| c == '.' || c == '\n' || c.is_whitespace())
                            .find(|s| !s.is_empty())
                            .map(|s| s.trim().to_string());
                        if let Some(acc_name) = account_name {
                            find_account_by_name(acc_name.as_str())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                if let Some(account_name) = account_name {
                    return Some(CpiAccountInfo {
                        account_name,
                        account_local: anchor_context_info.anchor_context_arg_local,
                    });
                }
            }
        }
        None
    }

    pub fn check_cpi_context_variables_are_same(
        &self,
        from: &Local,
        to: &Local,
        visited: &mut HashSet<Local>,
    ) -> bool {
        if visited.contains(from) {
            return false;
        }
        visited.insert(*from);
        if to == from {
            return true;
        }
        if let Some(assignment_locals) = &self.transitive_assignment_reverse_map.get(from) {
            for assignment_local in assignment_locals.iter() {
                if self.check_cpi_context_variables_are_same(assignment_local, to, visited) {
                    return true;
                }
            }
            return false;
        }
        false
    }

    pub fn check_local_variables_are_same(&self, from: &Local, to: &Local) -> bool {
        let from_local_original = self.resolve_to_original_local(*from, &mut HashSet::new());
        let to_local_original = self.resolve_to_original_local(*to, &mut HashSet::new());
        from_local_original == to_local_original
    }

    pub fn takes_cpi_context(&self, args: &[Spanned<rustc_middle::mir::Operand>]) -> bool {
        args.iter().any(|arg| {
            if let rustc_middle::mir::Operand::Copy(place) | rustc_middle::mir::Operand::Move(place) = &arg.node
                && let Some(local) = place.as_local()
                && let Some(decl) = self.mir.local_decls().get(local)
            {
                DiagnoticItem::AnchorCpiContext.defid_is_type(self.cx.tcx, decl.ty.peel_refs())
            } else {
                false
            }
        })
    }

    /// Check if two locals come from the same CPI context account
    pub fn are_same_account(&self, local1: Local, local2: Local) -> bool {
        if let (Some(account1), Some(account2)) = (
            self.is_from_cpi_context(local1, None),
            self.is_from_cpi_context(local2, None),
        ) {
            account1.account_name == account2.account_name
        } else {
            false
        }
    }

    // Finds the accounts struct in a CPI context.
    pub fn find_cpi_accounts_struct(
        &self,
        account_stuct_local: &Local,
        visited: &mut HashSet<Local>,
    ) -> Option<Vec<Local>> {
        if let Some(accounts) = self.cpi_account_local_map.get(account_stuct_local) {
            return Some(accounts.clone());
        }
        if visited.contains(account_stuct_local) {
            return None;
        }
        visited.insert(*account_stuct_local);
        for (lhs, rhs) in &self.reverse_assignment_map {
            if rhs.contains(account_stuct_local) {
                // recursively check the lhs
                if let Some(accounts) = self.find_cpi_accounts_struct(lhs, visited) {
                    return Some(accounts);
                }
            }
        }
        None
    }
}

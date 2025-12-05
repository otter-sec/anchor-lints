use clippy_utils::{source::HasSession, ty::is_type_diagnostic_item};
use rustc_middle::{
    mir::{HasLocalDecls, Local},
    ty::TyKind,
};
use rustc_span::source_map::Spanned;
use rustc_span::sym;

use std::collections::HashSet;

use super::types::MirAnalyzer;
use crate::models::*;
use crate::utils::{
    extract_context_account, extract_vec_elements, extract_vec_snippet_from_span, remove_comments,
};

impl<'cx, 'tcx> MirAnalyzer<'cx, 'tcx> {
    // Collects the accounts from the account_infos argument.
    pub fn collect_accounts_from_account_infos_arg(
        &self,
        arg: &Spanned<rustc_middle::mir::Operand<'tcx>>,
        return_only_name: bool,
    ) -> Vec<AccountNameAndLocal> {
        if let rustc_middle::mir::Operand::Copy(place) | rustc_middle::mir::Operand::Move(place) =
            arg.node
            && let Some(vec_local) = place.as_local()
            && let Some(vec_ty) = self
                .mir
                .local_decls()
                .get(vec_local)
                .map(|d| d.ty.peel_refs())
            && (is_type_diagnostic_item(self.cx, vec_ty, sym::Vec)
                || matches!(vec_ty.kind(), TyKind::Slice(_)))
        {
            return self.get_vec_elements(&vec_local, &mut HashSet::new(), return_only_name);
        }
        Vec::new()
    }

    pub fn get_vec_elements(
        &self,
        local: &Local,
        visited_locals: &mut HashSet<Local>,
        return_only_name: bool,
    ) -> Vec<AccountNameAndLocal> {
        let mut elements = Vec::new();
        if let Some(span) = self.get_span_from_local(local) {
            if visited_locals.contains(local) {
                if let Some(method_call_receiver) = self.method_call_receiver_map.get(local) {
                    return self.get_vec_elements(
                        method_call_receiver,
                        visited_locals,
                        return_only_name,
                    );
                }
                return elements;
            }
            visited_locals.insert(*local);
            let mut cleaned_snippet = String::new();
            if let Some(full_vec) = extract_vec_snippet_from_span(self.cx, span) {
                cleaned_snippet = remove_comments(&full_vec);
            } else if let Ok(snippet) = self.cx.tcx.sess().source_map().span_to_snippet(span) {
                cleaned_snippet = remove_comments(&snippet);
            }
            for element in extract_vec_elements(&cleaned_snippet) {
                if let Some(account_name) = extract_context_account(&element, return_only_name) {
                    elements.push(AccountNameAndLocal {
                        account_name,
                        account_local: *local,
                    });
                }
            }
            if !elements.is_empty() {
                return elements;
            }
            let resolved_local = self.resolve_to_original_local(*local, &mut HashSet::new());
            return self.get_vec_elements(&resolved_local, visited_locals, return_only_name);
        }

        elements
    }

    // Checks if a local is an account name and returns the account name and local.
    pub fn check_local_and_assignment_locals(
        &self,
        account_local: &Local,
        visited: &mut HashSet<Local>,
        return_only_name: bool,
        maybe_account_name: &mut String,
    ) -> Vec<AccountNameAndLocal> {
        let local_decl = &self.mir.local_decls[*account_local];
        let span = local_decl.source_info.span;
        let mut results = Vec::new();
        if let Ok(snippet) = self.cx.sess().source_map().span_to_snippet(span) {
            let cleaned_snippet = remove_comments(&snippet);
            if cleaned_snippet.trim_start().contains("vec!") {
                for element in extract_vec_elements(&cleaned_snippet) {
                    if let Some(account_name) = extract_context_account(&element, return_only_name)
                    {
                        results.push(AccountNameAndLocal {
                            account_name,
                            account_local: *account_local,
                        });
                    }
                }
                return results;
            }
            if let Some(account_name) = extract_context_account(&cleaned_snippet, return_only_name)
            {
                if cleaned_snippet.contains("accounts.") {
                    results.push(AccountNameAndLocal {
                        account_name,
                        account_local: *account_local,
                    });
                    return results;
                }
                *maybe_account_name = account_name;
            }
            if let Ok(file_span) = self.cx.sess().source_map().span_to_lines(span) {
                let file = &file_span.file;
                let start_line_idx = file_span.lines[0].line_index;
                if let Some(src) = file.src.as_ref() {
                    let lines: Vec<&str> = src.lines().collect();
                    if let Some(account_name) =
                        extract_context_account(lines[start_line_idx], return_only_name)
                    {
                        if lines[start_line_idx].contains("accounts.") {
                            results.push(AccountNameAndLocal {
                                account_name,
                                account_local: *account_local,
                            });
                            return results;
                        }
                        *maybe_account_name = account_name;
                    }
                }
            }
        }
        if visited.contains(account_local) {
            if !maybe_account_name.is_empty() && return_only_name {
                results.push(AccountNameAndLocal {
                    account_name: maybe_account_name.clone(),
                    account_local: *account_local,
                });
                return results;
            }
            return results;
        }
        visited.insert(*account_local);

        // First, check if this is a method call result
        if let Some(receiver_local) = self.method_call_receiver_map.get(account_local)
            && let account_name_and_locals = self.check_local_and_assignment_locals(
                receiver_local,
                visited,
                return_only_name,
                maybe_account_name,
            )
            && !account_name_and_locals.is_empty()
        {
            return account_name_and_locals;
        }

        // Then check assignment map
        for (lhs, rhs) in &self.transitive_assignment_reverse_map {
            if rhs.contains(account_local)
                && let account_name_and_locals = self.check_local_and_assignment_locals(
                    lhs,
                    visited,
                    return_only_name,
                    maybe_account_name,
                )
                && !account_name_and_locals.is_empty()
            {
                return account_name_and_locals;
            }
        }
        if !maybe_account_name.is_empty() && return_only_name {
            results.push(AccountNameAndLocal {
                account_name: maybe_account_name.clone(),
                account_local: *account_local,
            });
            return results;
        }
        results
    }
}

use clippy_utils::source::HasSession;
use rustc_lint::LateContext;
use rustc_middle::mir::{Body as MirBody, HasLocalDecls, Local, Operand};
use rustc_span::source_map::Spanned;
use std::collections::{HashMap, HashSet};

use crate::models::*;
use crate::utils::{extract_account_name_from_string, extract_context_account, remove_comments};

// Checks if a local is an account name and returns the account name and local.
pub fn check_local_and_assignment_locals<'tcx>(
    lookup_ctx: &AccountLookupContext<'_, 'tcx>,
    account_local: &Local,
    visited: &mut HashSet<Local>,
    return_only_name: bool,
    maybe_account_name: &mut String,
) -> Option<AccountNameAndLocal> {
    let local_decl = &lookup_ctx.mir.local_decls[*account_local];
    let span = local_decl.source_info.span;
    if let Ok(snippet) = lookup_ctx.cx.sess().source_map().span_to_snippet(span) {
        let cleaned_snippet = remove_comments(&snippet);
        if let Some(account_name) = extract_context_account(&cleaned_snippet, return_only_name) {
            if cleaned_snippet.contains("accounts.") {
                return Some(AccountNameAndLocal {
                    account_name,
                    account_local: *account_local,
                });
            }
            *maybe_account_name = account_name;
        }
        if let Ok(file_span) = lookup_ctx.cx.sess().source_map().span_to_lines(span) {
            let file = &file_span.file;
            let start_line_idx = file_span.lines[0].line_index;
            if let Some(src) = file.src.as_ref() {
                let lines: Vec<&str> = src.lines().collect();
                if let Some(account_name) =
                    extract_context_account(lines[start_line_idx], return_only_name)
                {
                    if lines[start_line_idx].contains("accounts.") {
                        return Some(AccountNameAndLocal {
                            account_name,
                            account_local: *account_local,
                        });
                    }
                    *maybe_account_name = account_name;
                }
            }
        }
    }
    if visited.contains(account_local) {
        if !maybe_account_name.is_empty() && return_only_name {
            return Some(AccountNameAndLocal {
                account_name: maybe_account_name.clone(),
                account_local: *account_local,
            });
        }
        return None;
    }
    visited.insert(*account_local);

    // First, check if this is a method call result
    if let Some(receiver_local) = lookup_ctx.method_call_receiver_map.get(account_local)
        && let Some(account_name_and_local) = check_local_and_assignment_locals(
            lookup_ctx,
            receiver_local,
            visited,
            return_only_name,
            maybe_account_name,
        )
    {
        return Some(account_name_and_local);
    }

    // Then check assignment map (for regular assignments like _4 = _3)
    for (lhs, rhs) in lookup_ctx.transitive_assignment_reverse_map {
        if rhs.contains(account_local) {
            // recursively check the lhs
            if let Some(account_name_and_local) = check_local_and_assignment_locals(
                lookup_ctx,
                lhs,
                visited,
                return_only_name,
                maybe_account_name,
            ) {
                return Some(account_name_and_local);
            }
        }
    }
    if !maybe_account_name.is_empty() && return_only_name {
        return Some(AccountNameAndLocal {
            account_name: maybe_account_name.clone(),
            account_local: *account_local,
        });
    }
    None
}

// Finds the accounts struct in a CPI context.
pub fn find_cpi_accounts_struct(
    account_stuct_local: &Local,
    reverse_assignment_map: &HashMap<Local, Vec<Local>>,
    cpi_accounts_map: &HashMap<Local, Vec<Local>>,
    visited: &mut HashSet<Local>,
) -> Option<Vec<Local>> {
    if let Some(accounts) = cpi_accounts_map.get(account_stuct_local) {
        return Some(accounts.clone());
    }
    if visited.contains(account_stuct_local) {
        return None;
    }
    visited.insert(*account_stuct_local);
    for (lhs, rhs) in reverse_assignment_map {
        if rhs.contains(account_stuct_local) {
            // recursively check the lhs
            if let Some(accounts) =
                find_cpi_accounts_struct(lhs, reverse_assignment_map, cpi_accounts_map, visited)
            {
                return Some(accounts);
            }
        }
    }
    None
}

// Extracts argumments if they contains context/context.accounts/context.accounts.account as arguments
pub fn get_nested_fn_arguments<'tcx>(
    cx: &LateContext<'tcx>,
    mir: &MirBody<'tcx>,
    args: &[Spanned<Operand>],
    cpi_context_info: &AnchorContextInfo<'tcx>,
) -> Option<NestedArgument<'tcx>> {
    let mut nested_argument = NestedArgument {
        arg_type: NestedArgumentType::Ctx,
        accounts: HashMap::new(),
    };
    let mut found = false;
    for (arg_index, arg) in args.iter().enumerate() {
        if let Operand::Move(place) | Operand::Copy(place) = &arg.node
            && let Some(local) = place.as_local()
            && let Some(account_ty) = mir.local_decls().get(local).map(|d| d.ty.peel_refs())
        {
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
                .find(|(_, accty)| *accty == &account_ty)
            {
                if let Ok(snippet) = cx.sess().source_map().span_to_snippet(arg.span) {
                    let cleaned_snippet = remove_comments(&snippet);
                    if let Some(acc_name) = extract_account_name_from_string(&cleaned_snippet) {
                        nested_argument.accounts.insert(
                            acc_name.clone(),
                            NestedAccount {
                                account_ty,
                                account_local: Local::from_usize(arg_index + 1),
                            },
                        );
                    }
                } else {
                    nested_argument.accounts.insert(
                        account_name.clone(),
                        NestedAccount {
                            account_ty,
                            account_local: Local::from_usize(arg_index + 1),
                        },
                    );
                }
                nested_argument.arg_type = NestedArgumentType::Account;
                found = true;
            }
        }
    }
    if found { Some(nested_argument) } else { None }
}

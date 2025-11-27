use clippy_utils::source::HasSession;
use rustc_lint::LateContext;
use rustc_middle::{
    mir::{BasicBlock, Body as MirBody, HasLocalDecls, Local, Operand},
    ty::Ty,
};
use rustc_span::source_map::Spanned;

use std::collections::{HashMap, HashSet};

use crate::models::*;
use crate::utils::{
    extract_account_name_from_string, extract_context_account, extract_vec_elements,
    normalize_account_name, remove_comments,
};

// Checks if a local is an account name and returns the account name and local.
pub fn check_local_and_assignment_locals<'tcx>(
    lookup_ctx: &AccountLookupContext<'_, 'tcx>,
    account_local: &Local,
    visited: &mut HashSet<Local>,
    return_only_name: bool,
    maybe_account_name: &mut String,
) -> Vec<AccountNameAndLocal> {
    let local_decl = &lookup_ctx.mir.local_decls[*account_local];
    let span = local_decl.source_info.span;
    let mut results = Vec::new();
    if let Ok(snippet) = lookup_ctx.cx.sess().source_map().span_to_snippet(span) {
        let cleaned_snippet = remove_comments(&snippet);
        if cleaned_snippet.trim_start().contains("vec!") {
            for element in extract_vec_elements(&cleaned_snippet) {
                if let Some(account_name) = extract_context_account(&element, return_only_name) {
                    results.push(AccountNameAndLocal {
                        account_name,
                        account_local: *account_local,
                    });
                }
            }
            return results;
        }
        if let Some(account_name) = extract_context_account(&cleaned_snippet, return_only_name) {
            if cleaned_snippet.contains("accounts.") {
                results.push(AccountNameAndLocal {
                    account_name,
                    account_local: *account_local,
                });
                return results;
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
    if let Some(receiver_local) = lookup_ctx.method_call_receiver_map.get(account_local)
        && let account_name_and_locals = check_local_and_assignment_locals(
            lookup_ctx,
            receiver_local,
            visited,
            return_only_name,
            maybe_account_name,
        )
        && !account_name_and_locals.is_empty()
    {
        return account_name_and_locals;
    }

    // Then check assignment map (for regular assignments like _4 = _3)
    for (lhs, rhs) in lookup_ctx.transitive_assignment_reverse_map {
        if rhs.contains(account_local)
            && let account_name_and_locals = check_local_and_assignment_locals(
                lookup_ctx,
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
                .find(|(_, accty)| *accty == &account_ty || is_account_info_type(cx, account_ty))
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

// Helper to check if a type is AccountInfo
fn is_account_info_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    if let rustc_middle::ty::TyKind::Adt(adt_def, _) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        def_path.contains("anchor_lang::prelude::AccountInfo")
            || def_path == "solana_program::account_info::AccountInfo"
    } else {
        false
    }
}

pub fn filter_account_accesses<'tcx>(
    cx: &LateContext<'tcx>,
    account_accesses: HashMap<String, Vec<AccountAccess>>,
    anchor_context_info: &AnchorContextInfo<'tcx>,
    cpi_accounts: &HashMap<String, BasicBlock>,
) -> HashMap<String, Vec<AccountAccess>> {
    let mut filtered_accesses = HashMap::new();
    for (name, accesses) in account_accesses {
        let normalized_name = normalize_account_name(&name);

        let (account_ty, contains_data) = if let Some(account_ty) = anchor_context_info
            .anchor_context_arg_accounts_type
            .get(normalized_name)
        {
            (
                Some(*account_ty),
                contains_deserialized_data(cx, *account_ty),
            )
        } else {
            (None, false)
        };

        let in_cpi = cpi_accounts.contains_key(&name) || cpi_accounts.contains_key(normalized_name);
        let should_flag = account_ty.map(|_| contains_data).unwrap_or(in_cpi);

        if should_flag {
            filtered_accesses.insert(name, accesses);
        }
    }
    filtered_accesses
}

// Checks if an account type contains deserialized data that needs reloading after CPI
pub fn contains_deserialized_data<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();
    if let rustc_middle::ty::TyKind::Adt(adt_def, substs) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());

        if def_path.contains("anchor_lang::prelude::Account")
            && !def_path.contains("AccountInfo")
            && !def_path.contains("UncheckedAccount")
        {
            return true;
        }
        if def_path.contains("anchor_lang::prelude::AccountLoader") {
            return false;
        }
        if def_path.contains("anchor_lang::prelude::InterfaceAccount")
            || def_path == "anchor_lang::prelude::SystemAccount"
        {
            return true;
        }

        // Signer, UncheckedAccount, and AccountInfo don't contain deserialized data
        if def_path.contains("anchor_lang::prelude::Signer")
            || def_path.contains("anchor_lang::prelude::UncheckedAccount")
            || def_path.contains("anchor_lang::prelude::AccountInfo")
            || def_path == "solana_program::account_info::AccountInfo"
        {
            return false;
        }

        // Check for Box<T> wrapper
        if def_path.contains("alloc::boxed::Box") && substs.len() > 0 {
            let inner_ty = substs.type_at(0);
            return contains_deserialized_data(cx, inner_ty);
        }
    }
    false
}

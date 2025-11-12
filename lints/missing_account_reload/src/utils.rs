use clippy_utils::{source::HasSession, ty::is_type_diagnostic_item};
use rustc_hir::{Body as HirBody, PatKind, def_id::DefId};
use rustc_lint::LateContext;
use rustc_middle::mir::{
    BasicBlock, BasicBlocks, Body as MirBody, HasLocalDecls, Local, Operand, Place, Rvalue,
    StatementKind, TerminatorKind,
};
use rustc_middle::ty::{self as rustc_ty, Ty, TyKind};
use rustc_span::source_map::Spanned;

use rustc_span::Symbol;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::models::{AccountNameAndLocal, AnchorContextInfo, NestedArgument, NestedArgumentType};

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
                let arg_local = Local::from_usize(arg_index + 1);
                if let Ok(snippet) = cx.sess().source_map().span_to_snippet(arg.span) {
                    let cleaned_snippet = remove_comments(&snippet);
                    if let Some(acc_name) = extract_account_name_from_string(&cleaned_snippet) {
                        nested_argument
                            .accounts
                            .insert(acc_name.clone(), (account_ty, arg_local));
                    }
                } else {
                    nested_argument
                        .accounts
                        .insert(account_name.clone(), (account_ty, arg_local));
                }
                nested_argument.arg_type = NestedArgumentType::Account;
                found = true;
            }
        }
    }
    if found { Some(nested_argument) } else { None }
}

// Extracts anchor context details from the function parameters
pub fn get_anchor_context_accounts<'tcx>(
    cx: &LateContext<'tcx>,
    body: &HirBody,
) -> Option<AnchorContextInfo<'tcx>> {
    let params = body.params;
    for (param_index, param) in params.iter().enumerate() {
        let param_ty = cx.typeck_results().pat_ty(param.pat).peel_refs();
        if let TyKind::Adt(adt_def, generics) = param_ty.kind() {
            let struct_name = cx.tcx.def_path_str(adt_def.did());
            if struct_name.ends_with("anchor_lang::context::Context")
                || struct_name.ends_with("anchor_lang::prelude::Context")
            {
                let variant = adt_def.non_enum_variant();
                for field in &variant.fields {
                    let field_name = field.ident(cx.tcx).to_string();
                    let field_ty = field.ty(cx.tcx, generics);
                    if field_name == "accounts" {
                        let accounts_struct_ty = field_ty.peel_refs();
                        if let TyKind::Adt(accounts_adt_def, accounts_generics) =
                            accounts_struct_ty.kind()
                        {
                            let accounts_variant = accounts_adt_def.non_enum_variant();
                            let mut cpi_ctx_accounts = HashMap::new();
                            for account_field in &accounts_variant.fields {
                                let account_name = account_field.ident(cx.tcx).to_string();
                                let account_ty = account_field.ty(cx.tcx, accounts_generics);
                                cpi_ctx_accounts.insert(account_name, account_ty);
                            }
                            let param_name = match param.pat.kind {
                                PatKind::Binding(_, _, ident, _) => ident.name.as_str().to_string(),
                                _ => {
                                    // fallback to span
                                    if let Ok(snippet) =
                                        cx.sess().source_map().span_to_snippet(param.pat.span)
                                    {
                                        let cleaned_snippet = remove_comments(&snippet);
                                        cleaned_snippet
                                            .split(':')
                                            .next()
                                            .unwrap_or("_")
                                            .trim()
                                            .to_string()
                                    } else {
                                        format!("param_{}", param_index)
                                    }
                                }
                            };
                            let arg_local = Local::from_usize(param_index + 1);
                            return Some(AnchorContextInfo {
                                anchor_context_name: param_name,
                                anchor_context_account_type: accounts_struct_ty,
                                anchor_context_arg_local: arg_local,
                                anchor_context_type: param_ty,
                                anchor_context_arg_accounts_type: cpi_ctx_accounts,
                            });
                        }
                    }
                }
            }
        }
    }
    None
}

// Recursively checks nested functions for account reload operations and returns account names with their types.
pub fn check_nested_account_reloads<'tcx>(
    cx: &LateContext<'tcx>,
    fn_def_id: &DefId,
    fn_crate_name: &String,
    cpi_context_info: &AnchorContextInfo<'tcx>,
) -> HashMap<String, (Ty<'tcx>, Local)> {
    let account_reload_sym = Symbol::intern("AnchorAccountReload");
    let mut account_tys = HashMap::new();
    let mir_body = cx.tcx.optimized_mir(fn_def_id);
    for (_, bbdata) in mir_body.basic_blocks.iter_enumerated() {
        if let TerminatorKind::Call {
            func: Operand::Constant(func),
            args,
            ..
        } = &bbdata.terminator().kind
            && let rustc_ty::FnDef(def_id, _) = func.ty().kind()
        {
            let crate_name = cx.tcx.crate_name(def_id.krate).to_string();

            if let Some(diag_item) = cx.tcx.diagnostic_items(def_id.krate).id_to_name.get(def_id) {
                if *diag_item == account_reload_sym
                    && let Some(account) = args.first()
                    && let Operand::Move(account) = account.node
                    && let Some(local) = account.as_local()
                    && let Some(account_ty) =
                        mir_body.local_decls().get(local).map(|d| d.ty.peel_refs())
                {
                    let (_, reverse_assignment_map) = build_local_relationship_maps(mir_body);
                    let transitive_assignment_reverse_map =
                        build_transitive_reverse_map(&reverse_assignment_map);

                    if let Some(account_name_and_local) = check_local_and_assignment_locals(
                        cx,
                        mir_body,
                        &local,
                        &transitive_assignment_reverse_map,
                        &mut HashSet::new(),
                        true,
                    ) {
                        let arg_local = resolve_to_original_local(
                            &account_name_and_local.account_local,
                            &mut HashSet::new(),
                            &transitive_assignment_reverse_map,
                        );
                        account_tys
                            .insert(account_name_and_local.account_name, (account_ty, arg_local));
                    }
                }
            } else if crate_name == *fn_crate_name
                && let Some(nested_argument) =
                    get_nested_fn_arguments(cx, mir_body, args, cpi_context_info)
            {
                let nested_account_reloads =
                    check_nested_account_reloads(cx, def_id, fn_crate_name, cpi_context_info);
                let mut nested_account_reloads_clone = nested_account_reloads.clone();
                for (account_name, (account_ty, arg_local)) in
                    nested_account_reloads_clone.clone().into_iter()
                {
                    if nested_argument.arg_type == NestedArgumentType::Account {
                        for (nested_account_name, (nested_account_ty, nested_arg_local)) in
                            nested_argument.accounts.clone().into_iter()
                        {
                            if nested_account_ty == account_ty && arg_local == nested_arg_local {
                                nested_account_reloads_clone.remove(&account_name);
                                nested_account_reloads_clone
                                    .insert(nested_account_name.clone(), (account_ty, arg_local));
                            }
                        }
                    }
                }
                account_tys.extend(nested_account_reloads_clone);
            }
        }
    }
    account_tys
}

// Resolves the original local from a local in a transitive assignment map.
pub fn resolve_to_original_local(
    from_local: &Local,
    visited: &mut HashSet<Local>,
    reverse_assignment_map: &HashMap<Local, Vec<Local>>,
) -> Local {
    if visited.contains(from_local) {
        return *from_local;
    }
    visited.insert(*from_local);

    for (src_local, dest_locals) in reverse_assignment_map {
        if dest_locals.contains(from_local) {
            return resolve_to_original_local(src_local, visited, reverse_assignment_map);
        }
    }

    *from_local
}

// Checks if the function arguments contains a CPI context.
pub fn takes_cpi_context(
    cx: &LateContext<'_>,
    mir: &MirBody<'_>,
    args: &[Spanned<Operand>],
) -> bool {
    args.iter().any(|arg| {
        if let Operand::Copy(place) | Operand::Move(place) = &arg.node
            && let Some(local) = place.as_local()
            && let Some(decl) = mir.local_decls().get(local)
        {
            is_type_diagnostic_item(cx, decl.ty.peel_refs(), Symbol::intern("AnchorCpiContext"))
        } else {
            false
        }
    })
}

/// Finds blocks in `to` that are reachable from `from` nodes without passing through `without` nodes
/// Returns a list of `to` nodes with the `from` node they are reachable from
pub fn reachable_without_passing(
    graph: &BasicBlocks,
    from: HashSet<BasicBlock>,
    to: HashSet<BasicBlock>,
    without: HashSet<BasicBlock>,
) -> Vec<(BasicBlock, BasicBlock)> {
    let mut queue = VecDeque::new();
    // Map of nodes to the `from` block they are reachable from
    let mut origin = HashMap::new();
    let mut visited = HashSet::new();

    for &f in &from {
        origin.insert(f, f);
        visited.insert(f);
        queue.push_back(f);
    }

    while let Some(u) = queue.pop_front() {
        if without.contains(&u) {
            continue;
        }
        for succ in graph[u]
            .terminator
            .as_ref()
            .map(|t| t.successors().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            if without.contains(&succ) || visited.contains(&succ) {
                continue;
            }
            origin.insert(succ, origin[&u]);
            visited.insert(succ);
            queue.push_back(succ);
        }
    }

    to.into_iter()
        .filter_map(|bb| origin.get(&bb).map(|o| (bb, *o)))
        .collect()
}

// Extracts account name from a code snippet matching the pattern `accounts.<name>` or standalone `name`.
pub fn extract_account_name_from_string(s: &str) -> Option<String> {
    let s = s.trim_start_matches("&mut ").trim_start_matches("& ");

    if let Some(accounts_pos) = s.find(".accounts.") {
        let after_accounts = &s[accounts_pos + ".accounts.".len()..];

        let account_name: String = after_accounts
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();

        if !account_name.is_empty() {
            return Some(account_name);
        }
    }

    if let Some(accounts_pos) = s.find("accounts.")
        && (accounts_pos == 0 || s[..accounts_pos].ends_with('.'))
    {
        let after_accounts = &s[accounts_pos + "accounts.".len()..];
        let account_name: String = after_accounts
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();

        if !account_name.is_empty() {
            return Some(account_name);
        }
    }

    None
}

// Builds a map of local variables to the local variables they are assigned to.
pub fn build_local_relationship_maps<'tcx>(
    mir: &MirBody<'tcx>,
) -> (HashMap<Local, Vec<Local>>, HashMap<Local, Vec<Local>>) {
    let mut cpi_account_local_map: HashMap<Local, Vec<Local>> = HashMap::new();
    let mut reverse_assignment_map: HashMap<Local, Vec<Local>> = HashMap::new();

    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        for statement in &bbdata.statements {
            if let StatementKind::Assign(box (dest_place, rvalue)) = &statement.kind
                && let Some(dest_local) = dest_place.as_local()
            {
                if let Rvalue::Aggregate(_, field_operands) = rvalue {
                    for operand in field_operands {
                        if let Operand::Copy(field_place) | Operand::Move(field_place) = operand
                            && let Some(field_local) = field_place.as_local()
                        {
                            cpi_account_local_map
                                .entry(dest_local)
                                .or_default()
                                .push(field_local);
                        }
                    }
                }

                let mut record_mapping = |src_place: &Place<'tcx>| {
                    let src_local = src_place.local;
                    reverse_assignment_map
                        .entry(src_local)
                        .or_default()
                        .push(dest_local);
                };

                match rvalue {
                    Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) => record_mapping(src),
                    Rvalue::Ref(_, _, src) => record_mapping(src),
                    Rvalue::Cast(_, Operand::Copy(src) | Operand::Move(src), _) => {
                        record_mapping(src)
                    }
                    Rvalue::Aggregate(_, operands) => {
                        for operand in operands {
                            if let Operand::Copy(src) | Operand::Move(src) = operand {
                                record_mapping(src);
                            }
                        }
                    }
                    Rvalue::CopyForDeref(src) => record_mapping(src),
                    _ => {}
                }
            }
        }
    }

    (cpi_account_local_map, reverse_assignment_map)
}

// Checks if a block is reachable from another block.
pub fn reachable_blocks(graph: &BasicBlocks, from: BasicBlock, to: &HashSet<BasicBlock>) -> bool {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    visited.insert(from);
    queue.push_back(from);

    while let Some(u) = queue.pop_front() {
        if to.contains(&u) {
            return true;
        }
        for succ in graph[u]
            .terminator
            .as_ref()
            .map(|t| t.successors().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            if visited.contains(&succ) {
                continue;
            }
            visited.insert(succ);
            queue.push_back(succ);
        }
    }
    false
}

// Extracts the account name from a code snippet matching the pattern `accounts.<name>` or standalone `name`.
pub fn extract_context_account(line: &str, return_only_name: bool) -> Option<String> {
    let snippet = remove_comments(line);

    let snippet = snippet.trim_start_matches("&mut ").trim_start_matches("& ");

    if let Some(start) = snippet.find(".accounts.") {
        let prefix_start = snippet[..start]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let prefix = &snippet[prefix_start..start]; // e.g., "ctx"

        let rest = &snippet[start + ".accounts.".len()..];

        let account_name_end = rest
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        let account = &rest[..account_name_end];
        if return_only_name {
            Some(account.to_string())
        } else {
            Some(format!("{}.accounts.{}", prefix, account))
        }
    } else if snippet.contains("accounts.") && return_only_name {
        let after_accounts = &snippet[snippet.find("accounts.").unwrap() + "accounts.".len()..];
        let account_name_end = after_accounts
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(after_accounts.len());
        let account = &after_accounts[..account_name_end];

        Some(account.to_string())
    } else {
        let account_name: String = snippet
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();

        if !account_name.is_empty() && account_name == snippet.trim() && return_only_name {
            Some(account_name)
        } else {
            None
        }
    }
}

// Builds a transitive reverse map of local variables to the local variables they are assigned to.
pub fn build_transitive_reverse_map(
    direct_map: &HashMap<Local, Vec<Local>>,
) -> HashMap<Local, Vec<Local>> {
    let mut transitive_map: HashMap<Local, Vec<Local>> = HashMap::new();

    for (&src, dests) in direct_map {
        let mut visited = HashSet::new();
        let mut queue: VecDeque<Local> = VecDeque::from(dests.clone());

        while let Some(next) = queue.pop_front() {
            if visited.insert(next) {
                transitive_map.entry(src).or_default().push(next);

                if let Some(next_dests) = direct_map.get(&next) {
                    for &nd in next_dests {
                        queue.push_back(nd);
                    }
                }
            }
        }
    }

    for vec in transitive_map.values_mut() {
        vec.sort();
    }

    transitive_map
}

// Checks if a local is an account name and returns the account name and local.
pub fn check_local_and_assignment_locals<'tcx>(
    cx: &LateContext<'tcx>,
    mir: &MirBody<'_>,
    account_local: &Local,
    transitive_assignment_reverse_map: &HashMap<Local, Vec<Local>>,
    visited: &mut HashSet<Local>,
    return_only_name: bool,
) -> Option<AccountNameAndLocal> {
    let local_decl = &mir.local_decls[*account_local];
    let span = local_decl.source_info.span;
    if let Ok(snippet) = cx.sess().source_map().span_to_snippet(span) {
        let cleaned_snippet = remove_comments(&snippet);
        if let Some(account_name) = extract_context_account(&cleaned_snippet, return_only_name) {
            return Some(AccountNameAndLocal {
                account_name,
                account_local: *account_local,
            });
        } else if let Ok(file_span) = cx.sess().source_map().span_to_lines(span) {
            let file = &file_span.file;
            let start_line_idx = file_span.lines[0].line_index;
            if let Some(src) = file.src.as_ref() {
                let lines: Vec<&str> = src.lines().collect();
                if let Some(account_name) =
                    extract_context_account(lines[start_line_idx], return_only_name)
                {
                    return Some(AccountNameAndLocal {
                        account_name,
                        account_local: *account_local,
                    });
                }
            }
        }
    }
    if visited.contains(account_local) {
        return None;
    }
    visited.insert(*account_local);

    for (lhs, rhs) in transitive_assignment_reverse_map {
        if rhs.contains(account_local) {
            // recursively check the lhs
            if let Some(account_name_and_local) = check_local_and_assignment_locals(
                cx,
                mir,
                lhs,
                transitive_assignment_reverse_map,
                visited,
                false,
            ) {
                return Some(account_name_and_local);
            }
        }
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

// Removes code comments (both `//` and `/* */`) from a source code string.
pub fn remove_comments(code: &str) -> String {
    let without_single = code.split("//").next().unwrap_or(code);

    without_single
        .split("/*")
        .next()
        .unwrap_or(without_single)
        .trim()
        .to_string()
}

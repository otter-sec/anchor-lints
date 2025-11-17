use clippy_utils::{source::HasSession, ty::is_type_diagnostic_item};
use rustc_hir::{Body as HirBody, PatKind};
use rustc_lint::LateContext;
use rustc_middle::mir::{
    BasicBlock, BasicBlocks, Body as MirBody, HasLocalDecls, Local, Operand, Place, Rvalue,
    StatementKind,
};
use rustc_middle::ty::TyKind;
use rustc_span::source_map::Spanned;

use rustc_span::Symbol;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::models::*;

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
    if !s.is_empty() {
        return Some(s.to_string());
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
pub fn reachable_block(graph: &BasicBlocks, from: BasicBlock, to: BasicBlock) -> bool {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    visited.insert(from);
    queue.push_back(from);

    while let Some(u) = queue.pop_front() {
        if u == to {
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

// Checks if a HashSet of blocks is reachable from another block.
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
        if return_only_name && snippet.contains('.') {
            let account_name = snippet.split('.').next().unwrap().to_string();
            if account_name.contains(":") {
                return Some(account_name.split(':').next_back().unwrap().to_string());
            }
            return Some(account_name.trim().to_string());
        }
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
    maybe_account_name: &mut String,
) -> Option<AccountNameAndLocal> {
    let local_decl = &mir.local_decls[*account_local];
    let span = local_decl.source_info.span;
    if let Ok(snippet) = cx.sess().source_map().span_to_snippet(span) {
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
        if let Ok(file_span) = cx.sess().source_map().span_to_lines(span) {
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

    for (lhs, rhs) in transitive_assignment_reverse_map {
        if rhs.contains(account_local) {
            // recursively check the lhs
            if let Some(account_name_and_local) = check_local_and_assignment_locals(
                cx,
                mir,
                lhs,
                transitive_assignment_reverse_map,
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

// Checks if a data access is stale by checking if it is reachable from a reload.
pub fn check_stale_data_accesses<'tcx>(
    mir: &MirBody<'tcx>,
    nested_function_blocks: &mut Vec<NestedFunctionBlocks<'tcx>>,
) {
    let nested_function_reloads: Vec<NestedFunctionBlocks<'tcx>> = nested_function_blocks
        .iter()
        .filter(|block| block.block_type == NestedBlockType::Reload)
        .cloned()
        .collect();
    if nested_function_reloads.is_empty() {
        return;
    }
    for nested_account_access_block in nested_function_blocks.iter_mut() {
        if nested_account_access_block.block_type == NestedBlockType::Reload {
            continue;
        }
        let mut is_rechable_from_reload = false;
        for nested_account_reload_block in nested_function_reloads.iter() {
            if nested_account_reload_block.block_type == NestedBlockType::Access {
                continue;
            }
            if nested_account_access_block.account_name == nested_account_reload_block.account_name
                && nested_account_access_block.account_ty == nested_account_reload_block.account_ty
                && nested_account_access_block.account_block
                    != nested_account_reload_block.account_block
                && reachable_block(
                    &mir.basic_blocks,
                    nested_account_reload_block.account_block,
                    nested_account_access_block.account_block,
                )
            {
                is_rechable_from_reload = true;
                break;
            }
        }
        nested_account_access_block.stale_data_access = !is_rechable_from_reload;
    }
}

// Processes nested function blocks and adds them to account_reloads or account_accesses
pub fn process_nested_function_blocks<'tcx>(
    nested_function_blocks: Vec<NestedFunctionBlocks<'tcx>>,
    nested_argument: &NestedArgument<'tcx>,
    anchor_context_info: &AnchorContextInfo<'tcx>,
    bb: BasicBlock,
    account_reloads: &mut HashMap<String, HashSet<BasicBlock>>,
    account_accesses: &mut HashMap<String, Vec<AccountAccess>>,
) {
    for nested_function_block in nested_function_blocks.into_iter() {
        if nested_argument.arg_type == NestedArgumentType::Account {
            for (nested_account_name, nested_account) in
                nested_argument.accounts.clone().into_iter()
            {
                if nested_account.account_ty == nested_function_block.account_ty
                    && nested_account.account_local == nested_function_block.account_local
                {
                    let account_block_name = format!(
                        "{}.accounts.{}",
                        anchor_context_info.anchor_context_name, nested_account_name
                    );
                    add_nested_function_block(
                        account_block_name,
                        &nested_function_block,
                        bb,
                        account_reloads,
                        account_accesses,
                    );
                }
            }
        } else {
            let account_block_name = format!(
                "{}.accounts.{}",
                anchor_context_info.anchor_context_name, nested_function_block.account_name
            );
            add_nested_function_block(
                account_block_name,
                &nested_function_block,
                bb,
                account_reloads,
                account_accesses,
            );
        }
    }
}

// Helper function to add nested function block to account_reloads or account_accesses
pub fn add_nested_function_block<'tcx>(
    account_block_name: String,
    nested_function_block: &NestedFunctionBlocks<'tcx>,
    bb: BasicBlock,
    account_reloads: &mut HashMap<String, HashSet<BasicBlock>>,
    account_accesses: &mut HashMap<String, Vec<AccountAccess>>,
) {
    if nested_function_block.block_type == NestedBlockType::Reload {
        if nested_function_block.not_used_reload {
            return;
        }
        account_reloads
            .entry(account_block_name)
            .or_default()
            .insert(bb);
    } else {
        account_accesses
            .entry(account_block_name)
            .or_default()
            .push(AccountAccess {
                access_block: bb,
                access_span: nested_function_block.account_span,
                stale_data_access: nested_function_block.stale_data_access,
            });
    }
}

// Creates a CpiContextCreationBlock with appropriate cpi_context_local
pub fn create_cpi_context_creation_block(
    account_name_and_local: AccountNameAndLocal,
    cpi_context_block: BasicBlock,
    mir_body: &MirBody<'_>,
    transitive_assignment_reverse_map: &HashMap<Local, Vec<Local>>,
) -> Option<CpiContextCreationBlock> {
    let arg_local = resolve_to_original_local(
        &account_name_and_local.account_local,
        &mut HashSet::new(),
        transitive_assignment_reverse_map,
    );

    let cpi_context_local = if arg_local == account_name_and_local.account_local {
        let next_index = arg_local.as_usize() + 1;
        if next_index < mir_body.local_decls().len() {
            let next_local = Local::from_usize(next_index);
            resolve_to_original_local(
                &next_local,
                &mut HashSet::new(),
                transitive_assignment_reverse_map,
            )
        } else {
            return None;
        }
    } else {
        arg_local
    };

    Some(CpiContextCreationBlock {
        cpi_context_block,
        account_name: account_name_and_local.account_name,
        cpi_context_local,
    })
}

// Processes nested CPI context creation and adds them to cpi_accounts
pub fn process_nested_cpi_context_creation<'tcx>(
    nested_cpi_context_creation: Vec<CpiContextCreationBlock>,
    nested_argument: &NestedArgument<'tcx>,
    anchor_context_info: &AnchorContextInfo<'tcx>,
    bb: BasicBlock,
    cpi_accounts: &mut HashMap<String, BasicBlock>,
) {
    for cpi_context_creation in nested_cpi_context_creation {
        if nested_argument.arg_type == NestedArgumentType::Account {
            for (nested_account_name, nested_account) in nested_argument.accounts.clone() {
                if nested_account.account_local == cpi_context_creation.cpi_context_local {
                    let account_block_name = format!(
                        "{}.accounts.{}",
                        anchor_context_info.anchor_context_name, nested_account_name
                    );
                    cpi_accounts.insert(account_block_name, bb);
                }
            }
        } else {
            let account_block_name = format!(
                "{}.accounts.{}",
                anchor_context_info.anchor_context_name, cpi_context_creation.account_name
            );
            cpi_accounts.insert(account_block_name, bb);
        }
    }
}

pub fn remap_nested_function_blocks<'tcx>(
    nested_blocks: Vec<NestedFunctionBlocks<'tcx>>,
    nested_argument: &NestedArgument<'tcx>,
    bb: BasicBlock,
) -> Vec<NestedFunctionBlocks<'tcx>> {
    nested_blocks
        .into_iter()
        .map(|mut nested_block| {
            nested_block.account_block = bb;
            if nested_argument.arg_type == NestedArgumentType::Account {
                for (nested_account_name, nested_account) in nested_argument.accounts.iter() {
                    if nested_account.account_ty == nested_block.account_ty
                        && nested_account.account_local == nested_block.account_local
                    {
                        nested_block.account_name = nested_account_name.clone();
                    }
                }
            }
            nested_block
        })
        .collect()
}

pub fn merge_nested_cpi_context_creation<'tcx>(
    nested_cpi_context_creation: Vec<CpiContextCreationBlock>,
    nested_argument: &NestedArgument<'tcx>,
    cpi_context_creation: &mut Vec<CpiContextCreationBlock>,
) {
    for nested_context in nested_cpi_context_creation {
        if nested_argument.arg_type == NestedArgumentType::Account {
            for (nested_account_name, nested_account) in nested_argument.accounts.iter() {
                if nested_account.account_local == nested_context.cpi_context_local {
                    cpi_context_creation.push(CpiContextCreationBlock {
                        cpi_context_block: nested_context.cpi_context_block,
                        account_name: nested_account_name.clone(),
                        cpi_context_local: nested_context.cpi_context_local,
                    });
                }
            }
        } else {
            cpi_context_creation.push(nested_context);
        }
    }
}

pub fn mark_unused_nested_reloads<'tcx>(
    mir_body: &MirBody<'tcx>,
    nested_function_blocks: &mut [NestedFunctionBlocks<'tcx>],
    cpi_calls: &[CpiCallBlock],
) {
    for reload in nested_function_blocks.iter_mut() {
        if reload.block_type != NestedBlockType::Reload {
            continue;
        }
        reload.not_used_reload = !cpi_calls.iter().any(|cpi_call| {
            reachable_block(
                &mir_body.basic_blocks,
                cpi_call.cpi_call_block,
                reload.account_block,
            )
        });
    }
}

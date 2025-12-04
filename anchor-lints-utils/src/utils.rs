use clippy_utils::source::HasSession;

use rustc_hir::{Body as HirBody, ImplItemKind, ItemKind, Node, PatKind};
use rustc_lint::LateContext;
use rustc_middle::{
    mir::{
        Body as MirBody, HasLocalDecls, Local, Operand, Place, Rvalue, StatementKind,
        TerminatorKind,
    },
    ty::{self as rustc_ty, Ty, TyKind},
};
use rustc_span::{Span, Symbol};

use std::collections::{HashMap, HashSet, VecDeque};

use crate::mir_analyzer::AnchorContextInfo;
use crate::models::*;

// Remove comments from a code snippet
pub fn remove_comments(code: &str) -> String {
    code.lines()
        .filter(|line| !line.trim().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract parameter data from a HIR parameter
fn extract_param_data<'tcx>(
    cx: &LateContext<'tcx>,
    mir: &MirBody<'tcx>,
    param_index: usize,
    param: &rustc_hir::Param<'tcx>,
) -> Option<ParamData<'tcx>> {
    let param_local = Local::from_usize(param_index + 1);
    let param_ty = mir.local_decls().get(param_local)?.ty.peel_refs();
    let param_name = extract_param_name(cx, param, param_index);

    let (adt_def, struct_name) = if let TyKind::Adt(adt_def, generics) = param_ty.kind() {
        let struct_name = cx.tcx.def_path_str(adt_def.did());
        (Some((adt_def, generics)), Some(struct_name))
    } else {
        (None, None)
    };

    Some(ParamData {
        param_index,
        param_local,
        param_name,
        param_ty,
        adt_def,
        struct_name,
    })
}

/// Extract parameter name from HIR parameter
fn extract_param_name<'tcx>(
    cx: &LateContext<'tcx>,
    param: &rustc_hir::Param<'tcx>,
    param_index: usize,
) -> String {
    match param.pat.kind {
        PatKind::Binding(_, _, ident, _) => ident.name.as_str().to_string(),
        _ => {
            // fallback to span
            if let Ok(snippet) = cx.sess().source_map().span_to_snippet(param.pat.span) {
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
    }
}

/// Get param info from function body
pub fn get_param_info<'tcx>(
    cx: &LateContext<'tcx>,
    mir: &MirBody<'tcx>,
    body: &HirBody<'tcx>,
) -> Vec<ParamInfo<'tcx>> {
    if body.params.is_empty() {
        return Vec::new();
    }

    let mut param_info: Vec<ParamInfo<'tcx>> = Vec::new();

    for (param_index, param) in body.params.iter().enumerate() {
        let Some(param_data) = extract_param_data(cx, mir, param_index, param) else {
            continue;
        };

        if let Some(struct_name) = param_data.struct_name {
            // Only collect single Anchor account types
            if is_single_anchor_account_type(&struct_name) {
                param_info.push(ParamInfo {
                    param_index,
                    param_name: param_data.param_name,
                    param_local: param_data.param_local,
                    param_ty: param_data.param_ty,
                });
            }
        }
    }

    param_info
}

/// Check if a type is a single Anchor account type (not an accounts struct)
fn is_single_anchor_account_type(struct_name: &str) -> bool {
    // Exclude single account types
    struct_name.starts_with("anchor_lang::prelude::")
        || struct_name == "solana_program::account_info::AccountInfo"
}

/// Extract account fields from an Adt type
fn extract_account_fields_from_adt<'tcx>(
    cx: &LateContext<'tcx>,
    adt_def: &rustc_ty::AdtDef<'tcx>,
    generics: &rustc_ty::GenericArgsRef<'tcx>,
) -> HashMap<String, rustc_ty::Ty<'tcx>> {
    let variant = adt_def.non_enum_variant();
    let mut accounts = HashMap::new();
    for field in &variant.fields {
        let account_name = field.ident(cx.tcx).to_string();
        let account_ty = field.ty(cx.tcx, generics);
        accounts.insert(account_name, account_ty);
    }
    accounts
}

/// Check if a type is an Anchor Context type
fn is_anchor_context_type(struct_name: &str) -> bool {
    struct_name.ends_with("anchor_lang::context::Context")
        || struct_name.ends_with("anchor_lang::prelude::Context")
}

/// Extract accounts field from a Context type
fn extract_accounts_from_context<'tcx>(
    cx: &LateContext<'tcx>,
    adt_def: &rustc_ty::AdtDef<'tcx>,
    generics: &rustc_ty::GenericArgsRef<'tcx>,
) -> Option<(rustc_ty::Ty<'tcx>, HashMap<String, rustc_ty::Ty<'tcx>>)> {
    let variant = adt_def.non_enum_variant();
    for field in &variant.fields {
        let field_name = field.ident(cx.tcx).to_string();
        if field_name == "accounts" {
            let accounts_struct_ty = field.ty(cx.tcx, generics).peel_refs();
            if let TyKind::Adt(accounts_adt_def, accounts_generics) = accounts_struct_ty.kind() {
                let accounts =
                    extract_account_fields_from_adt(cx, accounts_adt_def, accounts_generics);
                return Some((accounts_struct_ty, accounts));
            }
        }
    }
    None
}

/// Get anchor context accounts from function body
pub fn get_anchor_context_accounts<'tcx>(
    cx: &LateContext<'tcx>,
    mir: &MirBody<'tcx>,
    body: &HirBody<'tcx>,
) -> Option<AnchorContextInfo<'tcx>> {
    if body.params.is_empty() {
        return None;
    }

    for (param_index, param) in body.params.iter().enumerate() {
        let Some(param_data) = extract_param_data(cx, mir, param_index, param) else {
            continue;
        };

        if let (Some((adt_def, generics)), Some(struct_name)) =
            (param_data.adt_def, param_data.struct_name)
            && is_anchor_context_type(&struct_name)
            && let Some((accounts_struct_ty, cpi_ctx_accounts)) =
                extract_accounts_from_context(cx, adt_def, generics)
        {
            return Some(AnchorContextInfo {
                anchor_context_name: param_data.param_name,
                anchor_context_account_type: accounts_struct_ty,
                anchor_context_arg_local: param_data.param_local,
                anchor_context_type: param_data.param_ty,
                anchor_context_arg_accounts_type: cpi_ctx_accounts,
            });
        }
    }
    None
}

/// Get context accounts from function body
pub fn get_context_accounts<'tcx>(
    cx: &LateContext<'tcx>,
    mir: &MirBody<'tcx>,
    body: &HirBody<'tcx>,
) -> Option<AnchorContextInfo<'tcx>> {
    if body.params.is_empty() {
        return None;
    }

    for (param_index, param) in body.params.iter().enumerate() {
        let Some(param_data) = extract_param_data(cx, mir, param_index, param) else {
            continue;
        };

        if let (Some((adt_def, generics)), Some(struct_name)) =
            (param_data.adt_def, param_data.struct_name)
        {
            // Skip single Anchor account types - we only want accounts structs
            if is_single_anchor_account_type(&struct_name) {
                continue;
            }

            // Skip if it's a Context type (use get_anchor_context_accounts for those)
            if is_anchor_context_type(&struct_name) {
                continue;
            }

            let cpi_ctx_accounts = extract_account_fields_from_adt(cx, adt_def, generics);

            // Only return if we found account fields (indicating it's an accounts struct)
            if !cpi_ctx_accounts.is_empty() {
                return Some(AnchorContextInfo {
                    anchor_context_name: param_data.param_name,
                    anchor_context_account_type: param_data.param_ty,
                    anchor_context_arg_local: param_data.param_local,
                    anchor_context_type: param_data.param_ty,
                    anchor_context_arg_accounts_type: cpi_ctx_accounts,
                });
            }
        }
    }
    None
}

/// Builds the analysis maps for the MIR body
pub fn build_mir_analysis_maps<'tcx>(mir: &MirBody<'tcx>) -> MirAnalysisMaps<'tcx> {
    let mut assignment_map: HashMap<Local, AssignmentKind<'tcx>> = HashMap::new();
    let mut reverse_assignment_map: HashMap<Local, Vec<Local>> = HashMap::new();
    let mut cpi_account_local_map: HashMap<Local, Vec<Local>> = HashMap::new();

    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        for statement in &bbdata.statements {
            if let StatementKind::Assign(box (dest_place, rvalue)) = &statement.kind
                && let Some(dest_local) = dest_place.as_local()
            {
                // 1️⃣ AssignmentKind classification
                let kind = match rvalue {
                    Rvalue::Use(Operand::Constant(_)) => AssignmentKind::Const,
                    Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) => {
                        AssignmentKind::FromPlace(*src)
                    }
                    Rvalue::Ref(_, _, src_place) => AssignmentKind::RefTo(*src_place),
                    _ => AssignmentKind::Other,
                };
                assignment_map.insert(dest_local, kind);

                // Helper closure used for reverse mapping
                let mut record_mapping = |src_place: &Place<'tcx>| {
                    reverse_assignment_map
                        .entry(src_place.local)
                        .or_default()
                        .push(dest_local);
                };

                // 2️⃣ CPI map only for Aggregates
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

                // 3️⃣ Reverse mapping for all rvalue types
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

    MirAnalysisMaps {
        assignment_map,
        reverse_assignment_map,
        cpi_account_local_map,
    }
}

/// Build transitive reverse map from direct reverse map
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

// Builds a map of method call destinations to their receivers.
pub fn build_method_call_receiver_map<'tcx>(mir: &MirBody<'tcx>) -> HashMap<Local, Local> {
    let mut method_call_map: HashMap<Local, Local> = HashMap::new();

    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        if let Some(terminator) = &bbdata.terminator
            && let TerminatorKind::Call {
                func: _,
                args,
                destination,
                ..
            } = &terminator.kind
            && let Some(receiver) = args.first()
            && let Operand::Copy(receiver_place) | Operand::Move(receiver_place) = &receiver.node
            && let Some(receiver_local) = receiver_place.as_local()
            && let dest_place = destination
            && let Some(dest_local) = dest_place.as_local()
        {
            method_call_map.insert(dest_local, receiver_local);
        }
    }

    method_call_map
}

// Extracts account name from a code snippet matching the pattern `accounts.<name>` or standalone `name`.
pub fn extract_account_name_from_string(s: &str) -> Option<String> {
    let s = s.trim_start_matches("&mut ").trim_start_matches("& ");

    if let Some(after_accounts) = s.split(".accounts.").nth(1) {
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

    let dot_count = s.matches('.').count();
    match dot_count {
        1 => {
            if let Some(dot_pos) = s.find('.') {
                let before_dot = &s[..dot_pos];
                let account_name: String = before_dot
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !account_name.is_empty() {
                    return Some(account_name);
                }
            }
        }
        2 => {
            if let Some(last_dot_pos) = s.rfind('.') {
                let after_last_dot = &s[last_dot_pos + 1..];
                let account_name: String = after_last_dot
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !account_name.is_empty() {
                    return Some(account_name);
                }
            }
        }
        _ => {}
    }

    if !s.is_empty() {
        Some(s.to_string())
    } else {
        None
    }
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
        let prefix = &snippet[prefix_start..start];

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
    } else {
        extract_account_name_from_string(snippet)
    }
}

// Extracts the elements of a vec from a code snippet.
pub fn extract_vec_elements(snippet: &str) -> Vec<String> {
    let mut trimmed = snippet.trim();
    trimmed = trimmed
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .trim();

    // Find the actual vec! macro even if preceded by `let ... =`
    let (pos, open, close) = if let Some(idx) = trimmed.find("vec![") {
        (idx, "vec![", ']')
    } else if let Some(idx) = trimmed.find("vec!(") {
        (idx, "vec!(", ')')
    } else {
        return Vec::new();
    };

    let after_open = &trimmed[pos + open.len()..];

    // Find the matching closing bracket for this vec![] by tracking bracket depth
    let mut depth = 1; // We're already inside the opening bracket
    let mut close_pos = None;

    for (i, ch) in after_open.char_indices() {
        match ch {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' if ch == close => {
                depth -= 1;
                if depth == 0 {
                    close_pos = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    // Extract only the inner content up to the matching closing bracket
    let inner = if let Some(close_idx) = close_pos {
        &after_open[..close_idx]
    } else {
        // Fallback: try to trim end if we can't find matching bracket
        after_open
            .trim_end_matches(';')
            .trim_end_matches(close)
            .trim()
    };

    let mut elements = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for ch in inner.chars() {
        match ch {
            '[' | '(' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ']' | ')' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                if !current.trim().is_empty() {
                    elements.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    // Add the last element
    if !current.trim().is_empty() {
        elements.push(current.trim().to_string());
    }

    elements
}

pub fn extract_vec_snippet_from_span(cx: &LateContext<'_>, span: Span) -> Option<String> {
    let file_lines = cx.sess().source_map().span_to_lines(span).ok()?;
    let file = &file_lines.file;
    let start = file_lines.lines[0].line_index;

    let src = file.src.as_ref()?;
    let lines: Vec<&str> = src.lines().collect();

    let mut buf = String::new();
    let mut depth = 0;
    let mut seen_open = false;

    for line in lines.iter().skip(start) {
        buf.push_str(line);
        buf.push('\n');

        for ch in line.chars() {
            match ch {
                '[' | '(' | '{' => {
                    if !seen_open {
                        seen_open = true;
                    }
                    depth += 1;
                }
                ']' | ')' | '}' => {
                    if depth > 0 {
                        depth -= 1;
                    }
                }
                _ => {}
            }
        }

        if seen_open && depth == 0 {
            break;
        }
    }

    Some(buf)
}

/// Get HIR body from a LocalDefId, handling both Item and ImplItem cases
pub fn get_hir_body_from_local_def_id<'tcx>(
    cx: &LateContext<'tcx>,
    local_def_id: rustc_hir::def_id::LocalDefId,
) -> Option<rustc_hir::BodyId> {
    let hir_id = cx.tcx.local_def_id_to_hir_id(local_def_id);
    match cx.tcx.hir_node(hir_id) {
        Node::Item(item) => {
            if let ItemKind::Fn { body, .. } = &item.kind {
                Some(*body)
            } else {
                None
            }
        }
        Node::ImplItem(impl_item) => {
            if let ImplItemKind::Fn(_, body_id) = &impl_item.kind {
                Some(*body_id)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Check if a type is Option<UncheckedAccount>
pub fn is_option_unchecked_account_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, substs) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        if (def_path == "core::option::Option" || def_path == "std::option::Option")
            && let Some(inner_ty) = substs.types().next()
        {
            return is_unchecked_account_type(cx, inner_ty);
        }
    }
    false
}

/// Check if a type is UncheckedAccount
pub fn is_unchecked_account_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, _) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        return def_path == "anchor_lang::prelude::UncheckedAccount";
    }
    false
}

/// Extract account constraints from Anchor attributes
pub fn extract_account_constraints<'tcx>(
    cx: &LateContext<'tcx>,
    account_field: &rustc_middle::ty::FieldDef,
) -> AccountConstraint {
    let mut account_constraints = AccountConstraint {
        mutable: false,
        has_address_constraint: false,
        constraints: Vec::new(),
    };

    let tcx = cx.tcx;
    let attrs = tcx.get_all_attrs(account_field.did);
    let mut last_ident_constraint = false;
    let mut latest_account_constraint = String::new();

    for attr in attrs {
        if let rustc_hir::Attribute::Unparsed(_) = attr {
            let attr_item = attr.get_normal_item();
            if let rustc_hir::AttrArgs::Delimited(delim_args) = &attr_item.args {
                delim_args.tokens.iter().for_each(|token| if let rustc_ast::tokenstream::TokenTree::Token(token, _) = token { match token.kind {
                    rustc_ast::token::TokenKind::Ident(ident, ..) => {
                        if ident == Symbol::intern("mut") {
                            account_constraints.mutable = true;
                        } else if ident == Symbol::intern("constraint") {
                            last_ident_constraint = true;
                        } else if ident == Symbol::intern("address") {
                            account_constraints.has_address_constraint = true;
                        } else if last_ident_constraint {
                            latest_account_constraint =
                                latest_account_constraint.clone() + &ident.to_string();
                        }
                    }
                    rustc_ast::token::TokenKind::Comma => {
                        if last_ident_constraint {
                            last_ident_constraint = false;
                            if !latest_account_constraint.is_empty() {
                                account_constraints
                                    .constraints
                                    .push(latest_account_constraint.clone());
                                latest_account_constraint = String::new();
                            }
                        }
                    }
                    rustc_ast::token::TokenKind::Dot => {
                        if last_ident_constraint {
                            latest_account_constraint.push('.');
                        }
                    }
                    rustc_ast::token::TokenKind::Ne => {
                        if last_ident_constraint {
                            latest_account_constraint.push_str("!=");
                        }
                    }
                    rustc_ast::token::TokenKind::Eq => {
                        if !latest_account_constraint.is_empty() {
                            latest_account_constraint.push('=');
                        }
                    }
                    _ => {
                        // Ignore other token kinds
                    }
                } });
            }
        }
    }

    account_constraints
}


/// Check if an account is a PDA (has seeds constraint or address constraint pointing to a PDA)
pub fn is_pda_account<'tcx>(
    cx: &LateContext<'tcx>,
    account_field: &rustc_middle::ty::FieldDef,
) -> Option<PdaSigner> {
    let tcx = cx.tcx;
    let attrs = tcx.get_all_attrs(account_field.did);
    let account_name = account_field.ident(tcx).to_string();
    let account_span = tcx.def_span(account_field.did);

    let mut has_seeds = false;
    let mut seeds = Vec::new();
    let mut has_address = false;

    for attr in attrs {
        if let rustc_hir::Attribute::Unparsed(_) = attr {
            let attr_item = attr.get_normal_item();
            if let rustc_hir::AttrArgs::Delimited(delim_args) = &attr_item.args {
                let mut last_ident_seeds = false;
                for token in delim_args.tokens.iter() {
                    if let rustc_ast::tokenstream::TokenTree::Token(token, _) = token {
                        match token.kind {
                            rustc_ast::token::TokenKind::Ident(ident, ..) => {
                                if ident == Symbol::intern("seeds") {
                                    last_ident_seeds = true;
                                    has_seeds = true;
                                } else if ident == Symbol::intern("address") {
                                    has_address = true;
                                } else if last_ident_seeds {
                                    seeds.push(ident.to_string());
                                }
                            }
                            rustc_ast::token::TokenKind::Comma => {
                                last_ident_seeds = false;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // PDA accounts typically have either seeds or address constraint pointing to a const PDA
    if has_seeds || has_address {
        return Some(PdaSigner {
            account_name,
            account_span,
            has_seeds,
            seeds,
        });
    }

    None
}

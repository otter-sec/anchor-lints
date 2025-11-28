use clippy_utils::source::HasSession;

use rustc_hir::{Body as HirBody, PatKind};
use rustc_lint::LateContext;
use rustc_middle::{
    mir::{
        Body as MirBody, HasLocalDecls, Local, Operand, Place, Rvalue, StatementKind,
        TerminatorKind,
    },
    ty::TyKind,
};
use rustc_span::Span;

use std::collections::{HashMap, HashSet, VecDeque};

use crate::mir_analyzer::AnchorContextInfo;
use crate::models::{AssignmentKind, MirAnalysisMaps};

pub fn remove_comments(code: &str) -> String {
    code.lines()
        .filter(|line| !line.trim().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Get anchor context accounts from function body
pub fn get_anchor_context_accounts<'tcx>(
    cx: &LateContext<'tcx>,
    mir: &MirBody<'tcx>,
    body: &HirBody<'tcx>,
) -> Option<AnchorContextInfo<'tcx>> {
    let params = body.params;
    if params.is_empty() {
        return None;
    }
    for (param_index, param) in params.iter().enumerate() {
        // Get the type from MIR instead of typeck_results
        let param_local = Local::from_usize(param_index + 1);
        let param_ty = match mir.local_decls().get(param_local) {
            Some(decl) => decl.ty.peel_refs(),
            None => continue,
        };
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

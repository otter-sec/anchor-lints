use anchor_lints_utils::diag_items::anchor_inner_account_type;
use anchor_lints_utils::mir_analyzer::{AnchorContextInfo, MirAnalyzer};
use clippy_utils::source::HasSession;
use rustc_lint::LateContext;
use rustc_middle::mir::{
    Body as MirBody, Local, Operand, Place, ProjectionElem, Rvalue, StatementKind, VarDebugInfo,
};
use rustc_middle::ty::{Ty, TyKind};

use crate::utils::types::InitAccountInfo;

use std::collections::HashMap;

/// Build a map from MIR Local to variable name using debug info.
pub fn build_local_to_name_map<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
) -> HashMap<Local, String> {
    let mut map = HashMap::new();
    for var_debug_info in &mir_analyzer.mir.var_debug_info {
        if let VarDebugInfo {
            name,
            value: rustc_middle::mir::VarDebugInfoContents::Place(place),
            ..
        } = var_debug_info
        {
            // Only map direct locals (no projections)
            if place.projection.is_empty() {
                map.insert(place.local, name.to_string());
            }
        }
    }

    map
}

/// Maps field index in Accounts struct -> account name.
fn build_accounts_field_index_map<'tcx>(
    cx: &LateContext<'tcx>,
    anchor_context: &AnchorContextInfo<'tcx>,
) -> HashMap<usize, String> {
    let mut map = HashMap::new();

    let accounts_ty = anchor_context.anchor_context_account_type;
    let TyKind::Adt(adt, _) = accounts_ty.kind() else {
        return map;
    };

    let variant = adt.non_enum_variant();

    for (idx, field) in variant.fields.iter().enumerate() {
        let name = field.ident(cx.tcx).to_string();
        map.insert(idx, name);
    }
    map
}

/// Tracks locals that are aliases of `ctx.accounts.<account>`.
pub fn build_local_account_alias_map<'cx, 'tcx>(
    cx: &LateContext<'tcx>,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    anchor_context: &AnchorContextInfo<'tcx>,
    init_accounts: &HashMap<String, InitAccountInfo<'tcx>>,
) -> HashMap<Local, String> {
    let mir = mir_analyzer.mir;
    let mut alias_map = HashMap::new();

    let field_index_map = build_accounts_field_index_map(cx, anchor_context);

    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        for stmt in &bbdata.statements {
            let StatementKind::Assign(box (lhs, rvalue)) = &stmt.kind else {
                continue;
            };

            let Some(lhs_local) = lhs.as_local() else {
                continue;
            };

            let rhs_place = match rvalue {
                Rvalue::Use(Operand::Copy(p))
                | Rvalue::Use(Operand::Move(p))
                | Rvalue::Ref(_, _, p) => p,
                _ => continue,
            };

            if let Some(account) =
                extract_account_from_rhs_place(rhs_place, &field_index_map, init_accounts)
            {
                alias_map.insert(lhs_local, account);
            }
        }
    }
    alias_map
}

/// Extract account name from a right-hand-side place that accesses `ctx.accounts.<account>`.
fn extract_account_from_rhs_place<'tcx>(
    rhs: &Place<'tcx>,
    field_index_map: &HashMap<usize, String>,
    init_accounts: &HashMap<String, InitAccountInfo<'tcx>>,
) -> Option<String> {
    for proj in rhs.projection.iter() {
        if let ProjectionElem::Field(field, _) = proj {
            let idx = field.index();

            if let Some(name) = field_index_map.get(&idx)
                && init_accounts.contains_key(name)
            {
                return Some(name.clone());
            }
        }
    }

    None
}

/// Extract the base local and field index from a place that represents a field write.
pub fn extract_field_write_info(place: &Place<'_>) -> Option<(Local, usize)> {
    let base_local = place.local;
    let mut field_idx = None;
    for proj in place.projection.iter() {
        match proj {
            ProjectionElem::Deref => {
                // Continue through derefs
            }
            ProjectionElem::Field(field, _ty) => {
                field_idx = Some(field.index());
            }
            _ => return None,
        }
    }

    let field_idx = field_idx?;

    Some((base_local, field_idx))
}

/// Resolve the base account name from a MIR local.
pub fn resolve_base_account_name<'tcx>(
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
    base_local: Local,
    local_to_name: &HashMap<Local, String>,
    init_accounts: &HashMap<String, InitAccountInfo<'tcx>>,
    local_account_alias_map: &HashMap<Local, String>,
) -> Option<String> {
    // Try to get the name from debug info
    if let Some(name) = local_to_name.get(&base_local) {
        if init_accounts.contains_key(name) {
            return Some(name.clone());
        }
        for account_name in init_accounts.keys() {
            if name.starts_with(account_name) {
                return Some(account_name.clone());
            }
        }
    }
    if let Some(method_call_receiver) = mir_analyzer.method_call_receiver_map.get(&base_local)
        && let Some(account_name) = local_account_alias_map.get(method_call_receiver)
    {
        return Some(account_name.clone());
    }

    if let Some(account_name) =
        extract_account_name_from_local_span(mir_analyzer, base_local, init_accounts)
    {
        return Some(account_name);
    }

    // If the local is not in debug info, it might be a temporary created by dereferencing
    // Trace back through assignments to find the source
    if let Some(source_local) = trace_local_to_source(mir_analyzer.mir, base_local) {
        // Recursively resolve the source local
        return resolve_base_account_name(
            mir_analyzer,
            source_local,
            local_to_name,
            init_accounts,
            local_account_alias_map,
        );
    }

    // Fallback: try to resolve through the type
    let base_ty = mir_analyzer.mir.local_decls[base_local].ty;
    resolve_account_name_from_type(mir_analyzer.cx, base_ty, init_accounts)
}

/// Extract account name from the source span of a local.
fn extract_account_name_from_local_span<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    base_local: rustc_middle::mir::Local,
    init_accounts: &HashMap<String, InitAccountInfo<'tcx>>,
) -> Option<String> {
    let Some(anchor_context_info) = &mir_analyzer.anchor_context_info else {
        return None;
    };
    let anchor_context_name = &anchor_context_info.anchor_context_name;
    let sm = mir_analyzer.cx.sess().source_map();
    let span = mir_analyzer.mir.local_decls[base_local].source_info.span;
    let snippet = sm.span_to_snippet(span).ok()?;

    if snippet.split('.').count() == 4 {
        let mut parts = snippet.split('.');
        let first = parts.next().unwrap_or("");
        let second = parts.next().unwrap_or("");
        let third = parts.next().unwrap_or("");

        if first == anchor_context_name && second == "accounts" {
            return Some(third.to_string());
        }
    } else if snippet.split('.').count() == 3 {
        let mut parts = snippet.split('.');
        let first = parts.next().unwrap_or("");
        let second = parts.next().unwrap_or("");

        if first == anchor_context_name {
            return Some(second.to_string());
        }
    } else if let Some(init_account) =
        check_if_init_account_self_method(anchor_context_info, init_accounts)
    {
        return Some(init_account.0.clone());
    }

    None
}

/// Trace a local back to its source through assignments.
fn trace_local_to_source(mir: &MirBody<'_>, target_local: Local) -> Option<Local> {
    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        for stmt in &bbdata.statements {
            if let StatementKind::Assign(box (place, rvalue)) = &stmt.kind
                && place.local == target_local
                && place.projection.is_empty()
            {
                if let Some(source) = extract_source_local_from_rvalue(rvalue) {
                    return Some(source);
                }
                if let Some(source) = extract_source_from_field_projection(rvalue) {
                    return Some(source);
                }
            }
        }
        if let Some(terminator) = &bbdata.terminator
            && let rustc_middle::mir::TerminatorKind::Call {
                destination, args, ..
            } = &terminator.kind
            && destination.local == target_local
            && destination.projection.is_empty()
            && let Some(arg) = args.first()
            && let Some(source) = extract_local_from_operand(&arg.node)
        {
            return Some(source);
        }
    }

    None
}

/// Extract source local from field projection patterns like ctx.accounts.collection.
fn extract_source_from_field_projection(rvalue: &rustc_middle::mir::Rvalue<'_>) -> Option<Local> {
    use rustc_middle::mir::Rvalue;

    match rvalue {
        Rvalue::Ref(_, _, place) => {
            if !place.projection.is_empty() {
                Some(place.local)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract the source local from an Rvalue.
fn extract_source_local_from_rvalue(rvalue: &rustc_middle::mir::Rvalue<'_>) -> Option<Local> {
    use rustc_middle::mir::Rvalue;

    match rvalue {
        Rvalue::Ref(_, _, place) => Some(place.local),
        Rvalue::Use(operand) => extract_local_from_operand(operand),
        Rvalue::Cast(_, operand, _) => extract_local_from_operand(operand),

        _ => None,
    }
}

/// Extract local from an operand.
fn extract_local_from_operand(operand: &rustc_middle::mir::Operand<'_>) -> Option<Local> {
    use rustc_middle::mir::Operand;

    match operand {
        Operand::Copy(place) | Operand::Move(place) => Some(place.local),
        Operand::Constant(_) => None,
    }
}

/// Try to resolve account name by matching the inner type.
fn resolve_account_name_from_type<'tcx>(
    cx: &LateContext<'tcx>,
    ty: Ty<'tcx>,
    init_accounts: &HashMap<String, InitAccountInfo<'tcx>>,
) -> Option<String> {
    let inner_ty = anchor_inner_account_type(cx.tcx, ty)?;

    for (account_name, info) in init_accounts {
        if info.inner_ty == inner_ty {
            return Some(account_name.clone());
        }
    }

    None
}

/// Resolve struct field name from a type and field index.
pub fn resolve_struct_field_name<'tcx>(
    cx: &LateContext<'tcx>,
    ty: Ty<'tcx>,
    field_idx: usize,
) -> Option<String> {
    let ty = ty.peel_refs();
    let TyKind::Adt(adt, _) = ty.kind() else {
        return None;
    };
    if !adt.is_struct() && !adt.is_union() {
        return None;
    }

    let variant = adt.non_enum_variant();
    let field = variant.fields.iter().nth(field_idx)?;
    Some(field.ident(cx.tcx).to_string())
}

/// Check if the anchor context account type matches an init account.
pub(crate) fn check_if_init_account_self_method<'tcx>(
    anchor_context: &AnchorContextInfo<'tcx>,
    init_accounts: &HashMap<String, InitAccountInfo<'tcx>>,
) -> Option<(String, InitAccountInfo<'tcx>)> {
    for (account_name, info) in init_accounts {
        if info.inner_ty == anchor_context.anchor_context_account_type {
            return Some((account_name.clone(), info.clone()));
        }
    }
    None
}

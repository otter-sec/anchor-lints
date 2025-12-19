use crate::utils::field_analysis::{extract_inner_struct_fields, should_ignore_field};
use crate::utils::name_resolution::{
    build_local_account_alias_map, build_local_to_name_map, extract_field_write_info,
    resolve_base_account_name, resolve_struct_field_name,
};
use crate::utils::nested_functions::{
    analyze_nested_init_function, check_if_args_corresponds_to_init_accounts,
};
use crate::utils::types::InitAccountInfo;
use anchor_lints_utils::mir_analyzer::MirAnalyzer;
use rustc_hir::def_id::LocalDefId;
use rustc_lint::LateContext;
use rustc_middle::mir::{
    Body as MirBody, Local, Operand, Place, ProjectionElem, Rvalue, StatementKind, TerminatorKind,
};
use rustc_span::{Span, source_map::Spanned};

use std::collections::{HashMap, HashSet};

/// Collect all field assignments for accounts being initialized.
pub fn collect_account_field_assignments<'cx, 'tcx>(
    cx: &LateContext<'tcx>,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    def_id: LocalDefId,
    init_accounts: &HashMap<String, InitAccountInfo<'tcx>>,
    parent_fn_span: Span,
) -> HashMap<String, HashSet<String>> {
    let mut result: HashMap<String, HashSet<String>> = HashMap::new();
    let mir: &MirBody<'tcx> = mir_analyzer.mir;
    let local_account_alias_map = build_local_account_alias_map(
        cx,
        mir_analyzer,
        mir_analyzer.anchor_context_info.as_ref().unwrap(),
        init_accounts,
    );

    // Build a map from Local to variable name using debug info
    let local_to_name = build_local_to_name_map(mir_analyzer);

    // First, check for struct literal full assignments (handles **registrar = Registrar { ... })
    let struct_literal_assignments =
        detect_struct_literal_full_assignment(mir_analyzer, cx, init_accounts);
    for (account_name, fields) in struct_literal_assignments {
        result.entry(account_name).or_default().extend(fields);
    }

    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        for stmt in &bbdata.statements {
            let StatementKind::Assign(box (place, rvalue)) = &stmt.kind else {
                continue;
            };

            // Check for full struct assignments like *account = Struct::new(...)
            if let Some(account_name) = detect_full_struct_assignment(
                mir_analyzer,
                place,
                rvalue,
                &local_to_name,
                &local_account_alias_map,
                init_accounts,
            ) {
                // Mark all fields as initialized for this account
                if let Some(fields) =
                    extract_inner_struct_fields(cx, init_accounts[&account_name].inner_ty)
                {
                    for field in fields {
                        if !should_ignore_field(cx, &field) {
                            result
                                .entry(account_name.clone())
                                .or_default()
                                .insert(field.name);
                        }
                    }
                }
                continue;
            }

            // We only care about field writes: <base>.<field> = ...
            let Some((base_local, field_idx)) = extract_field_write_info(place) else {
                continue;
            };
            // Resolve struct field name
            let base_ty = mir_analyzer.mir.local_decls[base_local].ty;
            let Some(field_name) = resolve_struct_field_name(mir_analyzer.cx, base_ty, field_idx)
            else {
                continue;
            };
            // Get the variable name from the local
            let Some(base_name) = resolve_base_account_name(
                mir_analyzer,
                base_local,
                &local_to_name,
                init_accounts,
                &local_account_alias_map,
            ) else {
                continue;
            };
            // base = variable name like "collection"
            if init_accounts.contains_key(&base_name) {
                result.entry(base_name).or_default().insert(field_name);
            }
        }

        // Handle function calls (set_inner, nested functions, etc.)
        if let TerminatorKind::Call {
            func: Operand::Constant(func_const),
            args,
            ..
        } = &bbdata.terminator().kind
            && let rustc_middle::ty::FnDef(fn_def_id, _) = func_const.ty().kind()
        {
            let fn_path = mir_analyzer.cx.tcx.def_path_str(*fn_def_id);

            // Handle set_inner() calls: account.set_inner(struct)
            if fn_path.starts_with("anchor_lang::prelude::Account::")
                && fn_path.ends_with("::set_inner")
            {
                handle_set_inner_call(mir_analyzer, init_accounts, args, &mut result);
                continue;
            }

            // Handle same-crate nested helper functions
            let current_crate_name = cx.tcx.crate_name(def_id.to_def_id().krate).to_string();
            let callee_crate = cx.tcx.crate_name(fn_def_id.krate).to_string();
            if callee_crate == current_crate_name
                && let Some(_) = &mir_analyzer.anchor_context_info
            {
                let init_accounts_passed_to_nested_fn =
                    check_if_args_corresponds_to_init_accounts(mir_analyzer, args, init_accounts);
                let nested_fields = analyze_nested_init_function(
                    cx,
                    fn_def_id,
                    init_accounts,
                    init_accounts_passed_to_nested_fn.clone(),
                    parent_fn_span,
                );

                for (acc_name, fields) in nested_fields {
                    if init_accounts.contains_key(&acc_name) {
                        result
                            .entry(acc_name)
                            .or_default()
                            .extend(fields.into_iter());
                    }
                }
            }
        }
    }

    result
}

/// Detect full struct assignment via dereference: *local = Struct::new(...) or *local = Struct { ... }
fn detect_full_struct_assignment<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    place: &Place<'tcx>,
    rvalue: &Rvalue<'tcx>,
    local_to_name: &HashMap<Local, String>,
    local_account_alias_map: &HashMap<Local, String>,
    init_accounts: &HashMap<String, InitAccountInfo<'tcx>>,
) -> Option<String> {
    use rustc_middle::mir::Rvalue;

    // Check if assignment is to a dereferenced place: *local = ...
    let mut is_deref = false;
    let base_local = place.local;

    for proj in place.projection.iter() {
        match proj {
            ProjectionElem::Deref => {
                is_deref = true;
            }
            ProjectionElem::Field(_, _) => {
                // If there's a field projection, this is not a full struct assignment
                return None;
            }
            _ => {}
        }
    }
    if !is_deref {
        return None;
    }

    // Check if RHS is a struct constructor call or struct literal
    let is_struct_constructor = match rvalue {
        Rvalue::Aggregate(_, _) => true,
        Rvalue::Use(Operand::Copy(place) | Operand::Move(place)) => {
            // Check if this local was assigned from a constructor call
            if let Some(temp_local) = place.as_local() {
                check_if_local_from_constructor_call(mir_analyzer, temp_local)
            } else {
                false
            }
        }
        _ => false,
    };

    if !is_struct_constructor {
        return None;
    }

    // Resolve the account name from the base local
    resolve_base_account_name(
        mir_analyzer,
        base_local,
        local_to_name,
        init_accounts,
        local_account_alias_map,
    )
}

/// Check if a local was assigned from a constructor call (like Bank::new).
fn check_if_local_from_constructor_call<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    local: Local,
) -> bool {
    let mir = mir_analyzer.mir;

    // Check all basic blocks to see if this local is assigned from a constructor call
    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        if let Some(terminator) = &bbdata.terminator
            && let TerminatorKind::Call {
                func: Operand::Constant(func_const),
                destination,
                ..
            } = &terminator.kind
        {
            // Check if the destination is our local
            if destination.local == local && destination.projection.is_empty() {
                // Check if it's a constructor-like function
                if let rustc_middle::ty::FnDef(fn_def_id, _) = func_const.ty().kind() {
                    let fn_path = mir_analyzer.cx.tcx.def_path_str(*fn_def_id);
                    // Check if it looks like a constructor (ends with ::new)
                    if fn_path.ends_with("::new") || fn_path.contains("::new") {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Handle `set_inner()` calls: `account.set_inner(struct)`.
/// When `set_inner()` is called, all fields of the struct are considered initialized.
fn handle_set_inner_call<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    init_accounts: &HashMap<String, InitAccountInfo<'tcx>>,
    args: &[Spanned<Operand<'tcx>>],
    out: &mut HashMap<String, HashSet<String>>,
) {
    // Receiver is first arg: &mut Account<'info, T>
    let Some(receiver_arg) = args.first() else {
        return;
    };
    let (Operand::Copy(place) | Operand::Move(place)) = &receiver_arg.node else {
        return;
    };
    let Some(local) = place.as_local() else {
        return;
    };

    // Map local back to ctx.accounts.<name>
    let Some(acc_info) = mir_analyzer.extract_account_name_from_local(&local, true) else {
        return;
    };
    let raw_name = acc_info.account_name;
    let account_name = raw_name.split('.').next().unwrap_or(&raw_name).to_string();

    // Only care about #[account(init, ...)] accounts
    let Some(init_info) = init_accounts.get(&account_name) else {
        return;
    };

    // Get all fields of the inner struct type T
    let Some(fields) = extract_inner_struct_fields(mir_analyzer.cx, init_info.inner_ty) else {
        return;
    };

    let entry = out.entry(account_name).or_default();
    for f in fields {
        if !should_ignore_field(mir_analyzer.cx, &f) {
            entry.insert(f.name);
        }
    }
}

/// Detect when all fields of an account are assigned from a struct literal.
fn detect_struct_literal_full_assignment<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    cx: &LateContext<'tcx>,
    init_accounts: &HashMap<String, InitAccountInfo<'tcx>>,
) -> HashMap<String, HashSet<String>> {
    let mut result: HashMap<String, HashSet<String>> = HashMap::new();
    let mir = mir_analyzer.mir;

    // Track struct literals and which fields they contain
    let mut struct_literals: HashMap<Local, (rustc_middle::ty::Ty<'tcx>, HashSet<usize>)> =
        HashMap::new();
    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        for stmt in &bbdata.statements {
            if let StatementKind::Assign(box (place, rvalue)) = &stmt.kind
                && place.projection.is_empty()
                && let Rvalue::Aggregate(aggregate_kind, field_operands) = rvalue
                && let rustc_middle::mir::AggregateKind::Adt(adt_def, variant_idx, _, _, _) =
                    aggregate_kind.as_ref()
            {
                let adt_def = mir_analyzer.cx.tcx.adt_def(*adt_def);
                if adt_def.is_struct() {
                    let ty = mir_analyzer.mir.local_decls[place.local].ty;
                    use rustc_middle::ty::TyKind;
                    if let TyKind::Adt(_, _) = ty.peel_refs().kind() {
                        let ty_peeled = ty.peel_refs();
                        for account_info in init_accounts.values() {
                            let account_ty_peeled = account_info.inner_ty.peel_refs();
                            if let (TyKind::Adt(adt1, _), TyKind::Adt(adt2, _)) =
                                (ty_peeled.kind(), account_ty_peeled.kind())
                                && adt1.did() == adt2.did()
                            {
                                let mut present_fields = HashSet::new();
                                let variant = adt_def.variant(*variant_idx);
                                for (idx, _operand) in field_operands.iter().enumerate() {
                                    if idx < variant.fields.len() {
                                        present_fields.insert(idx);
                                    }
                                }
                                struct_literals.insert(place.local, (ty, present_fields));
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    if struct_literals.is_empty() {
        return result;
    }

    // For each account, check if all its fields are assigned from the same struct literal
    for (account_name, account_info) in init_accounts {
        if let Some(account_fields) = extract_inner_struct_fields(cx, account_info.inner_ty) {
            let non_ignored_fields: Vec<_> = account_fields
                .iter()
                .filter(|f| !should_ignore_field(cx, f))
                .collect();

            if non_ignored_fields.is_empty() {
                continue;
            }
            for (struct_literal_local, (struct_ty, present_field_indices)) in &struct_literals {
                use rustc_middle::ty::TyKind;
                let struct_ty = struct_ty.peel_refs();
                let account_ty = account_info.inner_ty.peel_refs();
                let types_match = match (struct_ty.kind(), account_ty.kind()) {
                    (TyKind::Adt(adt1, _), TyKind::Adt(adt2, _)) => adt1.did() == adt2.did(),
                    _ => false,
                };

                if !types_match {
                    continue;
                }

                let all_fields_present = non_ignored_fields.iter().all(|field| {
                    if let Some(field_idx) =
                        find_field_index_by_name(cx, account_info.inner_ty, &field.name)
                    {
                        present_field_indices.contains(&field_idx)
                    } else {
                        false
                    }
                });
                if all_fields_present {
                    // Now check if this struct literal is used to initialize the account
                    let mut is_used_for_account = false;
                    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
                        for stmt in &bbdata.statements {
                            if let StatementKind::Assign(box (_place, rvalue)) = &stmt.kind {
                                use rustc_middle::mir::{Operand, Rvalue};
                                if let Rvalue::Use(Operand::Copy(place) | Operand::Move(place)) =
                                    rvalue
                                    && place.local == *struct_literal_local
                                {
                                    is_used_for_account = true;
                                    break;
                                }
                            }
                        }
                        if is_used_for_account {
                            break;
                        }
                    }
                    if is_used_for_account {
                        // Mark all fields as assigned
                        for field in &non_ignored_fields {
                            result
                                .entry(account_name.clone())
                                .or_default()
                                .insert(field.name.clone());
                        }
                        break;
                    }
                }
            }
        }
    }

    result
}

/// Helper to find field index by name in a struct type.
fn find_field_index_by_name<'tcx>(
    cx: &LateContext<'tcx>,
    ty: rustc_middle::ty::Ty<'tcx>,
    field_name: &str,
) -> Option<usize> {
    use rustc_middle::ty::TyKind;
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, _generics) = ty.kind()
        && adt_def.is_struct()
    {
        let variant = adt_def.non_enum_variant();
        for (idx, field) in variant.fields.iter().enumerate() {
            if field.ident(cx.tcx).to_string() == field_name {
                return Some(idx);
            }
        }
    }
    None
}

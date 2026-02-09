extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use std::collections::HashSet;

use anchor_lints_utils::{
    diag_items::{is_anchor_account_type, is_pyth_get_price_no_older_than_fn, is_pyth_price_update_v2_type},
    mir_analyzer::{AnchorContextInfo, MirAnalyzer},
    utils::extract_arg_local,
};
use rustc_hir::def_id::DefId;
use rustc_lint::LateContext;
use rustc_middle::{
    mir::{Local, Operand, Place, ProjectionElem, Rvalue, StatementKind, TerminatorKind},
    ty::{self as rustc_ty, Ty, TyKind},
};
use rustc_span::{source_map::Spanned, sym};

/// Check account type is Account<PriceUpdateV2>
pub fn is_account_price_update_v2<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();

    match ty.kind() {
        TyKind::Adt(_adt_def, generics) => {
            if is_anchor_account_type(cx.tcx, ty)
                && let Some(inner_ty) = generics.types().next()
            {
                let inner_ty = inner_ty.peel_refs();
                if is_pyth_price_update_v2_type(cx.tcx, inner_ty) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

/// Check if a function is get_price_no_older_than
pub fn is_get_price_no_older_than<'tcx>(cx: &LateContext<'tcx>, fn_def_id: DefId) -> bool {
    is_pyth_get_price_no_older_than_fn(cx.tcx, fn_def_id)
}

/// Extract price account name from method call
pub fn extract_price_account_from_args<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    args: &[Spanned<Operand<'tcx>>],
) -> Option<String> {
    if let Some(receiver) = args.first()
        && let Operand::Copy(place) | Operand::Move(place) = &receiver.node
        && let Some(local) = place.as_local()
    {
        return mir_analyzer
            .extract_account_name_from_local(&local, false)
            .map(|info| info.account_name);
    }
    None
}

/// Check if account key is compared
pub fn has_pubkey_constant_check<'cx, 'tcx>(
    cx: &LateContext<'tcx>,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    account_name: &str,
) -> bool {
    let mir = mir_analyzer.mir;

    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        if let TerminatorKind::Call {
            func: Operand::Constant(func_const),
            args,
            ..
        } = &bbdata.terminator().kind
            && let rustc_ty::FnDef(fn_def_id, _) = func_const.ty().kind()
            && (cx.tcx.is_diagnostic_item(sym::cmp_partialeq_eq, *fn_def_id)
                || cx.tcx.is_diagnostic_item(sym::cmp_partialeq_ne, *fn_def_id))
        {
            for index in 0..args.len() {
                if let Some(local) = extract_arg_local(args, index) {
                    let compared_account_name =
                        mir_analyzer.extract_account_name_from_local(&local, true);

                    if let Some(compared_account_name) = compared_account_name
                        && (compared_account_name.account_name == account_name
                            || compared_account_name
                                .account_name
                                .starts_with(&format!("{}.", account_name)))
                    {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn extract_field_path(place: &Place<'_>) -> Vec<usize> {
    let mut path = Vec::new();
    for proj in place.projection.iter() {
        match proj {
            ProjectionElem::Deref => {
                // Skip derefs
            }
            ProjectionElem::Field(field, _) => {
                path.push(field.index());
            }
            _ => break,
        }
    }
    path
}

/// Check if a place represents price_message.publish_time
fn is_price_publish_time<'cx, 'tcx>(
    cx: &LateContext<'tcx>,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    place: &Place<'tcx>,
    account_name: &str,
) -> bool {
    let base_local = place.local;
    let field_path = extract_field_path(place);

    // Field path [2, 4] for price_message.publish_time
    if field_path != [2, 4] {
        return false;
    }

    if let Some(account_info) = mir_analyzer.extract_account_name_from_local(&base_local, true)
        && (account_info.account_name == account_name
            || account_info
                .account_name
                .starts_with(&format!("{}.", account_name)))
    {
        // Verify type is PriceUpdateV2
        let base_ty = mir_analyzer.mir.local_decls[base_local].ty.peel_refs();
        return is_pyth_price_update_v2_type(cx.tcx, base_ty);
    }
    false
}

fn is_state_last_publish_time<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    place: &Place<'tcx>,
) -> bool {
    let base_local = place.local;
    let field_path = extract_field_path(place);

    // Field path [0] for last_publish_time
    if field_path != [0] {
        return false;
    }

    let base_ty = mir_analyzer.mir.local_decls[base_local].ty.peel_refs();

    // Check if base_ty matches any account type from Anchor context
    if let Some(anchor_context_info) = &mir_analyzer.anchor_context_info {
        for account_ty in anchor_context_info
            .anchor_context_arg_accounts_type
            .values()
        {
            let account_ty_peeled = account_ty.peel_refs();

            // Extract inner type from Account<T>
            let inner_ty = if let TyKind::Adt(_, generics) = account_ty_peeled.kind() {
                generics.types().next().map(|inner| inner.peel_refs())
            } else {
                None
            };

            // Compare base_ty with the inner type
            if let Some(inner) = inner_ty {
                match (base_ty.kind(), inner.kind()) {
                    (TyKind::Adt(base_adt, _), TyKind::Adt(inner_adt, _)) => {
                        if base_adt.did() == inner_adt.did() {
                            return true;
                        }
                    }
                    _ => {
                        if base_ty == inner {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// Check if a local transitively represents a value from the tracked locals set
fn is_tracked_local<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    local: Local,
    tracked_locals: &HashSet<Local>,
) -> bool {
    // Direct check
    if tracked_locals.contains(&local) {
        return true;
    }

    // Check transitive assignments
    if let Some(sources) = mir_analyzer.transitive_assignment_reverse_map.get(&local) {
        for &src_local in sources {
            if tracked_locals.contains(&src_local) {
                return true;
            }
        }
    }

    false
}

/// Classify a place as publish_time or last_publish_time (direct field access or via tracked locals)
fn classify_place_for_comparison<'cx, 'tcx>(
    cx: &LateContext<'tcx>,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    place: &Place<'tcx>,
    account_name: &str,
    publish_time_locals: &HashSet<Local>,
    last_publish_time_locals: &HashSet<Local>,
) -> (bool, bool) {
    if let Some(local) = place.as_local() {
        (
            is_tracked_local(mir_analyzer, local, publish_time_locals),
            is_tracked_local(mir_analyzer, local, last_publish_time_locals),
        )
    } else {
        (
            is_price_publish_time(cx, mir_analyzer, place, account_name),
            is_state_last_publish_time(mir_analyzer, place),
        )
    }
}

/// Check if publish_time is stored and compared for monotonicity
pub fn has_monotonic_publish_time_enforcement<'cx, 'tcx>(
    cx: &LateContext<'tcx>,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    account_name: &str,
    _anchor_context_info: &AnchorContextInfo<'tcx>,
) -> bool {
    let mir = mir_analyzer.mir;
    let mut has_publish_time_comparison = false;
    let mut has_publish_time_store = false;

    let mut publish_time_locals = HashSet::new();
    let mut last_publish_time_locals = HashSet::new();

    // Identify publish_time and state account field locals
    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        for stmt in &bbdata.statements {
            if let StatementKind::Assign(box (dest_place, rvalue)) = &stmt.kind {
                // Check for publish_time field access
                if let Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) = rvalue {
                    if is_price_publish_time(cx, mir_analyzer, src, account_name)
                        && let Some(dest_local) = dest_place.as_local()
                    {
                        publish_time_locals.insert(dest_local);
                    }

                    // Check for state account field access
                    if is_state_last_publish_time(mir_analyzer, src)
                        && let Some(dest_local) = dest_place.as_local()
                    {
                        last_publish_time_locals.insert(dest_local);
                    }
                }

                // Check for assignments that copy publish_time or state account field
                if let Rvalue::Use(Operand::Copy(src_place) | Operand::Move(src_place)) = rvalue
                    && let Some(dest_local) = dest_place.as_local()
                    && let Some(src_local) = src_place.as_local()
                {
                    if is_tracked_local(mir_analyzer, src_local, &publish_time_locals) {
                        publish_time_locals.insert(dest_local);
                    }
                    if is_tracked_local(mir_analyzer, src_local, &last_publish_time_locals) {
                        last_publish_time_locals.insert(dest_local);
                    }
                }

                if let Rvalue::Use(Operand::Copy(src_place) | Operand::Move(src_place)) = rvalue
                    && is_state_last_publish_time(mir_analyzer, dest_place)
                    && let Some(src_local) = src_place.as_local()
                    && is_tracked_local(mir_analyzer, src_local, &publish_time_locals)
                {
                    has_publish_time_store = true;
                }
                // Check for comparison: publish_time > state account field
                if let Rvalue::BinaryOp(op, box (left, right)) = rvalue
                    && matches!(
                        op,
                        rustc_middle::mir::BinOp::Gt
                            | rustc_middle::mir::BinOp::Lt
                            | rustc_middle::mir::BinOp::Ge
                            | rustc_middle::mir::BinOp::Le
                    )
                    && let (
                        Operand::Copy(left_place) | Operand::Move(left_place),
                        Operand::Copy(right_place) | Operand::Move(right_place),
                    ) = (left, right)
                {
                    let (lhs_is_publish_time, lhs_is_last_publish_time) =
                        classify_place_for_comparison(
                            cx,
                            mir_analyzer,
                            left_place,
                            account_name,
                            &publish_time_locals,
                            &last_publish_time_locals,
                        );

                    let (rhs_is_publish_time, rhs_is_last_publish_time) =
                        classify_place_for_comparison(
                            cx,
                            mir_analyzer,
                            right_place,
                            account_name,
                            &publish_time_locals,
                            &last_publish_time_locals,
                        );

                    // Comparison must involve one publish_time and one state account field
                    if (lhs_is_publish_time && rhs_is_last_publish_time)
                        || (rhs_is_publish_time && lhs_is_last_publish_time)
                    {
                        has_publish_time_comparison = true;
                    }
                }
            }
        }
    }
    // Both comparison and storage are required for monotonicity enforcement
    has_publish_time_comparison && has_publish_time_store
}

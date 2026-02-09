use anchor_lints_utils::{
    diag_items::{
        is_anchor_account_type, is_anchor_key_fn, is_anchor_to_account_info_fn, is_borrow_fn,
        is_box_type, is_cpi_builder_constructor_fn, is_deserialize_fn,
    },
    mir_analyzer::MirAnalyzer,
    utils::account_constraints::extract_account_constraints,
};
use rustc_hir::def_id::DefId;
use rustc_lint::LateContext;
use rustc_middle::{
    mir::{Operand, TerminatorKind},
    ty::{self as rustc_ty, Ty, TyKind},
};
use rustc_span::{Span, Symbol};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct AccountInfo<'tcx> {
    pub name: String,
    pub span: Span,
    #[allow(unused)]
    pub ty: Ty<'tcx>,
    pub has_seeds: bool,
    pub has_address_constraint: bool,
    pub has_owner_constraint: bool,
    pub is_account_type: bool,
}

// extract accounts that need owner validation
pub fn extract_accounts_needing_owner_check<'tcx>(
    cx: &LateContext<'tcx>,
    anchor_context_info: &anchor_lints_utils::mir_analyzer::AnchorContextInfo<'tcx>,
) -> HashMap<String, AccountInfo<'tcx>> {
    let mut accounts_needing_check = HashMap::new();
    let accounts_struct_ty = &anchor_context_info.anchor_context_account_type;

    if let TyKind::Adt(accounts_adt_def, accounts_generics) = accounts_struct_ty.kind() {
        if !accounts_adt_def.is_struct() && !accounts_adt_def.is_union() {
            return HashMap::new();
        }
        let variant = accounts_adt_def.non_enum_variant();

        for account_field in &variant.fields {
            let account_name = account_field.ident(cx.tcx).to_string();
            let account_span = cx.tcx.def_span(account_field.did);
            let account_ty = account_field.ty(cx.tcx, accounts_generics);
            let inner_ty = unwrap_box_type(cx, account_ty);

            let constraints = extract_account_constraints(cx, account_field);
            let has_seeds = has_seeds_constraint(cx, account_field);
            let has_address = constraints.has_address_constraint;
            let has_owner = has_owner_constraint(cx, account_field);

            let is_account_type = is_anchor_account_type(cx.tcx, inner_ty);

            if !is_account_type {
                accounts_needing_check.insert(
                    account_name.clone(),
                    AccountInfo {
                        name: account_name,
                        span: account_span,
                        ty: inner_ty,
                        has_seeds,
                        has_address_constraint: has_address,
                        has_owner_constraint: has_owner,
                        is_account_type,
                    },
                );
            }
        }
    }

    accounts_needing_check
}

pub fn account_needs_owner_check(
    account_info: &AccountInfo,
    accounts_with_data_access: &HashSet<String>,
) -> bool {
    if account_info.has_seeds {
        return false;
    }
    if account_info.has_address_constraint {
        return false;
    }
    if account_info.has_owner_constraint {
        return false;
    }
    if account_info.is_account_type {
        return false;
    }

    if accounts_with_data_access.contains(&account_info.name) {
        return true;
    }

    false
}

fn has_seeds_constraint<'tcx>(
    cx: &LateContext<'tcx>,
    account_field: &rustc_middle::ty::FieldDef,
) -> bool {
    let attrs = cx.tcx.get_all_attrs(account_field.did);
    for attr in attrs {
        if let rustc_hir::Attribute::Unparsed(_) = attr {
            let attr_item = attr.get_normal_item();
            if let rustc_hir::AttrArgs::Delimited(delim_args) = &attr_item.args {
                for token in delim_args.tokens.iter() {
                    if let rustc_ast::tokenstream::TokenTree::Token(token, _) = token
                        && let rustc_ast::token::TokenKind::Ident(ident, ..) = token.kind
                        && ident == Symbol::intern("seeds")
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn has_owner_constraint<'tcx>(
    cx: &LateContext<'tcx>,
    account_field: &rustc_middle::ty::FieldDef,
) -> bool {
    let attrs = cx.tcx.get_all_attrs(account_field.did);
    for attr in attrs {
        if let rustc_hir::Attribute::Unparsed(_) = attr {
            let attr_item = attr.get_normal_item();
            if let rustc_hir::AttrArgs::Delimited(delim_args) = &attr_item.args {
                let mut found_owner = false;
                for token in delim_args.tokens.iter() {
                    if let rustc_ast::tokenstream::TokenTree::Token(token, _) = token {
                        match token.kind {
                            rustc_ast::token::TokenKind::Ident(ident, ..) => {
                                if ident == Symbol::intern("owner") {
                                    found_owner = true;
                                }
                            }
                            rustc_ast::token::TokenKind::Eq => {
                                if found_owner {
                                    return true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    false
}

fn unwrap_box_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> Ty<'tcx> {
    let ty = ty.peel_refs();
    if let TyKind::Adt(_adt_def, substs) = ty.kind() {
        if is_box_type(cx.tcx, ty) && !substs.is_empty() {
            return substs.type_at(0);
        }
    }
    ty
}

// extract accounts with data access
pub fn extract_accounts_with_data_access<'cx, 'tcx>(
    cx: &LateContext<'tcx>,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    anchor_context_info: &anchor_lints_utils::mir_analyzer::AnchorContextInfo<'tcx>,
) -> HashSet<String> {
    let mut accounts_with_data_access = HashSet::new();
    let mut accounts_used_as_cpi_programs = HashSet::new();
    let mir = mir_analyzer.mir;

    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        if let TerminatorKind::Call {
            func: Operand::Constant(func_const),
            args,
            fn_span: _,
            ..
        } = &bbdata.terminator().kind
            && let rustc_ty::FnDef(fn_def_id, _) = func_const.ty().kind()
        {
            // skip if the function is a deref method, to_account_info, or key
            if cx
                .tcx
                .is_diagnostic_item(rustc_span::sym::deref_method, *fn_def_id)
                || is_anchor_to_account_info_fn(cx.tcx, *fn_def_id)
                || is_anchor_key_fn(cx.tcx, *fn_def_id)
            {
                continue;
            }

            // skip if the function is a cpi builder constructor
            if is_cpi_builder_constructor_fn(cx.tcx, *fn_def_id) {
                for arg in args {
                    if let Operand::Copy(place) | Operand::Move(place) = &arg.node
                        && let Some(account_name) =
                            trace_account_from_place(mir_analyzer, place, anchor_context_info)
                    {
                        accounts_used_as_cpi_programs.insert(account_name);
                    }
                }
                continue;
            }

            // extract account name from deserialize or borrow
            if let Some(account_name) = extract_account_from_deserialize(
                cx,
                *fn_def_id,
                mir_analyzer,
                args,
                anchor_context_info,
            )
            .or_else(|| {
                extract_account_from_borrow(cx, *fn_def_id, mir_analyzer, args, anchor_context_info)
            }) {
                accounts_with_data_access.insert(account_name);
            }
        }
    }

    // return accounts with data access that are not used as cpi programs
    accounts_with_data_access
        .difference(&accounts_used_as_cpi_programs)
        .cloned()
        .collect()
}

// from borrow method
fn extract_account_from_borrow<'cx, 'tcx>(
    cx: &LateContext<'tcx>,
    fn_def_id: DefId,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    args: &[rustc_span::source_map::Spanned<Operand<'tcx>>],
    anchor_context_info: &anchor_lints_utils::mir_analyzer::AnchorContextInfo<'tcx>,
) -> Option<String> {
    if !is_borrow_fn(cx.tcx, fn_def_id) {
        return None;
    }
    let receiver = args.first()?;
    if let Operand::Copy(place) | Operand::Move(place) = &receiver.node {
        if let Some(name) = trace_account_from_place(mir_analyzer, place, anchor_context_info) {
            return Some(name);
        }
        if let Some(local) = place.as_local()
            && let Some(name) = trace_account_from_local(mir_analyzer, &local, anchor_context_info)
        {
            return Some(name);
        }
    }
    None
}

// from deserialize method
fn extract_account_from_deserialize<'cx, 'tcx>(
    cx: &LateContext<'tcx>,
    fn_def_id: DefId,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    args: &[rustc_span::source_map::Spanned<Operand<'tcx>>],
    anchor_context_info: &anchor_lints_utils::mir_analyzer::AnchorContextInfo<'tcx>,
) -> Option<String> {
    if is_deserialize_fn(cx.tcx, fn_def_id) {
        extract_account_name_from_call_args(mir_analyzer, args, anchor_context_info)
    } else {
        None
    }
}

fn trace_account_from_local<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    local: &rustc_middle::mir::Local,
    _anchor_context_info: &anchor_lints_utils::mir_analyzer::AnchorContextInfo<'tcx>,
) -> Option<String> {
    use std::collections::HashSet;

    let account_name_and_locals = mir_analyzer.check_local_and_assignment_locals(
        local,
        &mut HashSet::new(),
        true,
        &mut String::new(),
    );
    if let Some(first) = account_name_and_locals.first() {
        return Some(first.account_name.clone());
    }

    None
}

fn trace_account_from_place<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    place: &rustc_middle::mir::Place<'tcx>,
    anchor_context_info: &anchor_lints_utils::mir_analyzer::AnchorContextInfo<'tcx>,
) -> Option<String> {
    trace_account_from_local(mir_analyzer, &place.local, anchor_context_info)
}

fn extract_account_name_from_call_args<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    args: &[rustc_span::source_map::Spanned<Operand<'tcx>>],
    anchor_context_info: &anchor_lints_utils::mir_analyzer::AnchorContextInfo<'tcx>,
) -> Option<String> {
    for arg in args {
        if let Operand::Copy(place) | Operand::Move(place) = &arg.node {
            if let Some(account_name) =
                trace_account_from_place(mir_analyzer, place, anchor_context_info)
            {
                return Some(account_name);
            }
            if let Some(local) = place.as_local()
                && let Some(account_info) =
                    mir_analyzer.extract_account_name_from_local(&local, true)
            {
                return Some(account_info.account_name);
            }
        }
    }
    None
}

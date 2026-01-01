use anchor_lints_utils::{
    diag_items::is_anchor_cpi_context,
    mir_analyzer::MirAnalyzer,
    utils::{check_cpi_call_is_new_with_signer, check_locals_are_related, extract_arg_local},
};

use rustc_lint::LateContext;
use rustc_middle::{
    mir::{AggregateKind, BasicBlock, Local, Operand, Rvalue, StatementKind, TerminatorKind},
    ty::{self as rustc_ty, TyKind},
};
use rustc_span::{Span, Symbol, source_map::Spanned};
use std::collections::{HashMap, HashSet};

use crate::cpi_rules::CpiMeta;

// Insert the authority account name and span from the argument at the given index
pub fn insert_authority_from_arg(
    mir_analyzer: &MirAnalyzer,
    args: &[Spanned<Operand>],
    idx: usize,
    out: &mut HashMap<String, Span>,
) {
    if let Some(local) = extract_arg_local(args, idx)
        && let (Some(account_info), Some(span)) = (
            mir_analyzer.extract_account_name_from_local(&local, true),
            mir_analyzer.get_span_from_local(&local),
        )
    {
        out.insert(account_info.account_name.clone(), span);
    }
}

// Extract the signer accounts from the CPI context
pub fn extract_cpi_accounts_from_context(
    mir_analyzer: &MirAnalyzer,
    cpi_block: BasicBlock,
    cpi_ctx_local: Local,
    cpi_meta: &CpiMeta,
) -> HashMap<String, Span> {
    let mut signer_account_names = HashMap::new();
    // Iterate over the basic blocks of the MIR to find the CPI accounts until we reach the CPI block
    for (bb, bbdata) in mir_analyzer.mir.basic_blocks.iter_enumerated() {
        if let TerminatorKind::Call {
            func: Operand::Constant(func_const),
            args,
            destination,
            ..
        } = &bbdata.terminator().kind
            && let rustc_ty::FnDef(fn_def_id, _) = func_const.ty().kind()
        {
            // Get the return type of the function
            let fn_sig = mir_analyzer.cx.tcx.fn_sig(*fn_def_id).skip_binder();
            let return_ty = fn_sig.skip_binder().output();

            // Check if the function return type is a CPI context
            if !is_anchor_cpi_context(mir_analyzer.cx.tcx, return_ty) {
                continue;
            }

            // Skip if the CPI call is new_with_signer - PDA signer
            if check_cpi_call_is_new_with_signer(mir_analyzer, args, *fn_def_id) {
                continue;
            }

            // Extract the accounts passed to the CPI call
            if let Some(cpi_accounts_local) = extract_arg_local(args, 1)
                && let Some(destination_local) = destination.as_local()
                && check_locals_are_related(
                    &mir_analyzer.reverse_assignment_map,
                    &destination_local,
                    &cpi_ctx_local,
                )
            {
                signer_account_names.extend(extract_cpi_accounts_from_transfer(
                    mir_analyzer,
                    bb,
                    cpi_accounts_local,
                    cpi_meta,
                ));
                break;
            }
        }

        // Stop iterating over the basic blocks if we've reached the CPI block
        if cpi_block == bb {
            break;
        }
    }
    signer_account_names
}

// Extract the signer accounts from the CPI transfer
pub fn extract_cpi_accounts_from_transfer(
    mir_analyzer: &MirAnalyzer,
    cpi_accounts_block: BasicBlock,
    cpi_accounts_local: Local,
    cpi_meta: &CpiMeta,
) -> HashMap<String, Span> {
    let mut signer_account_names = HashMap::new();

    // Iterate over the basic blocks of the MIR to find the CPI accounts until we reach the CPI context block
    for (bb, bbdata) in mir_analyzer.mir.basic_blocks.iter_enumerated() {
        if let TerminatorKind::Call { .. } = &bbdata.terminator().kind {
            for stmt in &bbdata.statements {
                if let StatementKind::Assign(box (place, rvalue)) = &stmt.kind
                    && let Rvalue::Aggregate(box AggregateKind::Adt(adt_def_id, _, _, _, _), fields) =
                        rvalue
                {
                    let Some(account_struct_local) = place.as_local() else {
                        continue;
                    };

                    // Check if the account struct local is related to the CPI accounts local
                    if !check_locals_are_related(
                        &mir_analyzer.reverse_assignment_map,
                        &account_struct_local,
                        &cpi_accounts_local,
                    ) {
                        continue;
                    }

                    let span = stmt.source_info.span;

                    let adt_def = mir_analyzer.cx.tcx.adt_def(*adt_def_id);
                    if !adt_def.is_struct() && !adt_def.is_union() {
                        continue;
                    }
                    let variant = adt_def.non_enum_variant();

                    // Find the index of the signer field in the accounts struct
                    if let Some(field_index) = variant.fields.iter().position(|f| {
                        f.ident(mir_analyzer.cx.tcx).to_string() == cpi_meta.signer_field_name
                    }) {
                        // Iterate over the fields of the accounts struct
                        for (idx, field_operand) in fields.iter().enumerate() {
                            // Matches the index of the field with the index of the signer field
                            if idx == field_index {
                                if let Operand::Copy(p) | Operand::Move(p) = field_operand
                                    && let Some(local) = p.as_local()
                                    && let Some(account_info) =
                                        mir_analyzer.extract_account_name_from_local(&local, true)
                                {
                                    signer_account_names
                                        .insert(account_info.account_name.clone(), span);
                                }
                                break;
                            }
                        }
                    }

                    break;
                }
            }
        }
        // Stop iterating over the basic blocks if we've reached the CPI accounts block
        if cpi_accounts_block == bb {
            break;
        }
    }
    signer_account_names
}

// Extract the accounts with signer attribute/type from the anchor context
pub fn extract_accounts_with_signer_attribute<'tcx>(
    cx: &LateContext<'tcx>,
    anchor_context_info: &anchor_lints_utils::mir_analyzer::AnchorContextInfo<'tcx>,
) -> HashSet<String> {
    let mut accounts_with_signer = HashSet::new();
    let accounts_struct_ty = &anchor_context_info.anchor_context_account_type;

    if let TyKind::Adt(accounts_adt_def, _accounts_generics) = accounts_struct_ty.kind() {
        if !accounts_adt_def.is_struct() && !accounts_adt_def.is_union() {
            return HashSet::new();
        }
        let variant = accounts_adt_def.non_enum_variant();

        for account_field in &variant.fields {
            let account_name = account_field.ident(cx.tcx).to_string();
            let ty = cx.tcx.type_of(account_field.did).instantiate_identity();

            // 1. Detect `Signer<'info>`
            if is_signer_type(cx, ty) {
                accounts_with_signer.insert(account_name.clone());
                continue;
            }

            // 2. Check for #[account(signer)] attribute
            let attrs = cx.tcx.get_all_attrs(account_field.did);
            for attr in attrs {
                if let rustc_hir::Attribute::Unparsed(_) = attr {
                    let attr_item = attr.get_normal_item();
                    if let rustc_hir::AttrArgs::Delimited(delim_args) = &attr_item.args {
                        for token in delim_args.tokens.iter() {
                            if let rustc_ast::tokenstream::TokenTree::Token(token, _) = token
                                && let rustc_ast::token::TokenKind::Ident(ident, ..) = token.kind
                                && ident == Symbol::intern("signer")
                            {
                                accounts_with_signer.insert(account_name.clone());
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    accounts_with_signer
}

// Check if the type is a Signer<'info>
pub fn is_signer_type<'tcx>(cx: &LateContext<'tcx>, ty: rustc_ty::Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Adt(adt_def, _) => {
            let path = cx.tcx.def_path_str(adt_def.did());
            path == "anchor_lang::prelude::Signer"
        }
        _ => false,
    }
}

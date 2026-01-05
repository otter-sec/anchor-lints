use anchor_lints_utils::{
    mir_analyzer::{AnchorContextInfo, MirAnalyzer},
    utils::account_constraints::has_account_constraint,
};

use clippy_utils::source::HasSession;
use rustc_lint::LateContext;
use rustc_middle::ty::{Ty, TyKind};

/// Check if an instruction is an init or init_if_needed instruction
pub fn is_init_instruction<'tcx>(
    cx: &LateContext<'tcx>,
    anchor_context_info: &AnchorContextInfo<'tcx>,
) -> bool {
    let accounts_struct_ty = &anchor_context_info.anchor_context_account_type;

    if let TyKind::Adt(adt_def, generics) = accounts_struct_ty.kind() {
        if !adt_def.is_struct() && !adt_def.is_union() {
            return false;
        }

        let variant = adt_def.non_enum_variant();

        // Check if any account has init or init_if_needed constraint
        for field in &variant.fields {
            if has_account_constraint(cx, field, "init")
                || has_account_constraint(cx, field, "init_if_needed")
            {
                return true;
            }

            // Check for payer constraint
            if has_account_constraint(cx, field, "payer") {
                return true;
            }

            // Check for space constraint
            if has_account_constraint(cx, field, "space") {
                return true;
            }
        }

        // Check if system_program is required
        for field in &variant.fields {
            let account_ty = field.ty(cx.tcx, generics);
            if is_system_program_type(cx, account_ty) {
                return true;
            }
        }
    }

    false
}

/// Extract account names referenced in PDA seeds
pub fn extract_seed_accounts_from_pda<'tcx>(
    cx: &LateContext<'tcx>,
    pda_field: &rustc_middle::ty::FieldDef,
) -> Vec<String> {
    use rustc_span::Symbol;

    let mut seed_accounts = Vec::new();
    let attrs = cx.tcx.get_all_attrs(pda_field.did);
    let mut last_ident_seeds = false;

    for attr in attrs {
        if let rustc_hir::Attribute::Unparsed(_) = attr {
            let attr_item = attr.get_normal_item();
            if let rustc_hir::AttrArgs::Delimited(delim_args) = &attr_item.args {
                for token in delim_args.tokens.iter() {
                    match token {
                        rustc_ast::tokenstream::TokenTree::Token(token, _) => {
                            match token.kind {
                                rustc_ast::token::TokenKind::Ident(ident, ..) => {
                                    if ident == Symbol::intern("seeds") {
                                        last_ident_seeds = true;
                                    } else if last_ident_seeds {
                                        // Check if this is an account name (not a method like "key", "as_ref", etc.)
                                        let ident_str = ident.as_str();
                                        if ident_str != "key"
                                            && ident_str != "as_ref"
                                            && ident_str != "b"
                                            && ident_str != "bump"
                                            && !ident_str.starts_with('&')
                                        {
                                            seed_accounts.push(ident_str.to_string());
                                        }
                                    }
                                }
                                rustc_ast::token::TokenKind::Comma => {
                                    last_ident_seeds = false;
                                }
                                _ => {}
                            }
                        }
                        rustc_ast::tokenstream::TokenTree::Delimited(_, _, _, token_stream) => {
                            // Recursively extract account names from nested structures
                            let nested_accounts =
                                recursively_extract_seed_accounts(token_stream, last_ident_seeds);
                            seed_accounts.extend(nested_accounts);
                            last_ident_seeds = false;
                        }
                    }
                }
            }
        }
    }

    // Remove duplicates and return
    seed_accounts.sort();
    seed_accounts.dedup();
    seed_accounts
}

/// Check if a type is SystemProgram
pub fn is_system_program_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, _) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        return def_path.contains("SystemProgram") || def_path.contains("system_program");
    }
    false
}

/// Recursively extract account names from seed token streams
pub fn recursively_extract_seed_accounts(
    token_stream: &rustc_ast::tokenstream::TokenStream,
    in_seeds_context: bool,
) -> Vec<String> {
    let mut account_names = Vec::new();
    let mut potential_account: Option<String> = None;

    for token_tree in token_stream.iter() {
        match token_tree {
            rustc_ast::tokenstream::TokenTree::Token(token, _) => {
                match token.kind {
                    rustc_ast::token::TokenKind::Ident(ident, ..) => {
                        if in_seeds_context {
                            let ident_str = ident.as_str();
                            // Skip common methods and keywords
                            if ident_str == "key" || ident_str == "as_ref" || ident_str == "b" {
                                if let Some(account) = potential_account.take() {
                                    account_names.push(account);
                                }
                            } else if ident_str != "bump" && !ident_str.starts_with('&') {
                                potential_account = Some(ident_str.to_string());
                            }
                        }
                    }
                    rustc_ast::token::TokenKind::Dot => {
                        if let Some(account) = potential_account.take() {
                            account_names.push(account);
                        }
                    }
                    rustc_ast::token::TokenKind::Comma => {
                        if let Some(account) = potential_account.take() {
                            account_names.push(account);
                        }
                    }
                    _ => {}
                }
            }
            rustc_ast::tokenstream::TokenTree::Delimited(_, _, _, nested_stream) => {
                let nested_accounts =
                    recursively_extract_seed_accounts(nested_stream, in_seeds_context);
                account_names.extend(nested_accounts);
                potential_account = None;
            }
        }
    }

    if let Some(account) = potential_account {
        account_names.push(account);
    }

    account_names
}

/// Check if a type is SystemAccount
pub fn is_system_account_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, _) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        return def_path == "anchor_lang::prelude::SystemAccount"
            || def_path.contains("SystemAccount");
    }
    false
}

pub fn is_account_required<'cx, 'tcx>(
    cx: &'cx LateContext<'tcx>,
    field: &rustc_middle::ty::FieldDef,
    account_name: &str,
    anchor_context_info: &AnchorContextInfo<'tcx>,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
) -> bool {
    // Check if account is a fee payer
    if has_account_constraint(cx, field, "payer") {
        return true;
    }

    // Check if account is used as a close target
    if is_account_used_as_close_target(cx, anchor_context_info, account_name) {
        return true;
    }

    // Check if account is a signer
    if has_account_constraint(cx, field, "signer") {
        return true;
    }

    // Check if account is mutable - if mutable, assume it might be needed for balance changes and skip
    if has_account_constraint(cx, field, "mut") {
        return true;
    }

    // Check if account's lamports are accessed
    if is_account_lamports_accessed(mir_analyzer, account_name) {
        return true;
    }

    // Check if account is referenced anywhere in the instruction body
    if is_account_referenced_in_body(mir_analyzer, account_name) {
        return true;
    }

    false
}

pub fn is_account_used_as_close_target<'tcx>(
    cx: &LateContext<'tcx>,
    anchor_context_info: &AnchorContextInfo<'tcx>,
    account_name: &str,
) -> bool {
    let accounts_struct_ty = &anchor_context_info.anchor_context_account_type;

    if let TyKind::Adt(adt_def, _) = accounts_struct_ty.kind() {
        if !adt_def.is_struct() && !adt_def.is_union() {
            return false;
        }

        let variant = adt_def.non_enum_variant();

        for field in &variant.fields {
            let attrs = cx.tcx.get_all_attrs(field.did);

            for attr in attrs {
                if let rustc_hir::Attribute::Unparsed(_) = attr {
                    let attr_item = attr.get_normal_item();
                    if let rustc_hir::AttrArgs::Delimited(delim_args) = &attr_item.args {
                        let mut found_close = false;
                        let mut after_eq = false;

                        for token in delim_args.tokens.iter() {
                            if let rustc_ast::tokenstream::TokenTree::Token(token, _) = token {
                                match token.kind {
                                    rustc_ast::token::TokenKind::Ident(ident, ..) => {
                                        if ident.as_str() == "close" {
                                            found_close = true;
                                        } else if found_close && after_eq {
                                            // Check if this identifier matches account_name
                                            if ident.as_str() == account_name {
                                                return true;
                                            }
                                        }
                                    }
                                    rustc_ast::token::TokenKind::Eq => {
                                        if found_close {
                                            after_eq = true;
                                        }
                                    }
                                    _ => {
                                        if found_close
                                            && after_eq
                                            && let Ok(snippet) = cx
                                                .tcx
                                                .sess()
                                                .source_map()
                                                .span_to_snippet(token.span)
                                            && snippet.contains(account_name)
                                        {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    false
}

pub fn is_account_lamports_accessed<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    account_name: &str,
) -> bool {
    for (_bb, bbdata) in mir_analyzer.mir.basic_blocks.iter_enumerated() {
        for stmt in &bbdata.statements {
            let span = stmt.source_info.span;
            if !span.is_dummy()
                && let Ok(snippet) = mir_analyzer.cx.sess().source_map().span_to_snippet(span)
                && snippet.contains(account_name)
                && snippet.contains("lamports")
            {
                return true;
            }
        }

        if let Some(terminator) = &bbdata.terminator {
            let span = terminator.source_info.span;
            if !span.is_dummy()
                && let Ok(snippet) = mir_analyzer.cx.sess().source_map().span_to_snippet(span)
                && snippet.contains(account_name)
                && snippet.contains("lamports")
            {
                return true;
            }
        }
    }
    false
}

pub fn is_account_referenced_in_body<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    account_name: &str,
) -> bool {
    for (_bb, bbdata) in mir_analyzer.mir.basic_blocks.iter_enumerated() {
        // Check statements
        for stmt in &bbdata.statements {
            let span = stmt.source_info.span;
            if !span.is_dummy()
                && let Ok(snippet) = mir_analyzer.cx.sess().source_map().span_to_snippet(span)
            {
                // Check account is referenced and it's not just .key() or .as_ref()
                if snippet.contains(account_name) {
                    if !snippet.contains(".key()")
                        && !snippet.contains(".as_ref()")
                        && !snippet.contains("seeds")
                    {
                        return true;
                    }
                    if snippet.contains(&format!("{}.", account_name))
                        || snippet.contains(&format!("{} ", account_name))
                        || snippet.contains(&format!("{},", account_name))
                    {
                        return true;
                    }
                }
            }
        }

        // Check terminator
        if let Some(terminator) = &bbdata.terminator {
            let span = terminator.source_info.span;
            if !span.is_dummy()
                && let Ok(snippet) = mir_analyzer.cx.sess().source_map().span_to_snippet(span)
                && snippet.contains(account_name)
                && !snippet.contains(".key()")
                && !snippet.contains(".as_ref()")
            {
                return true;
            }
        }
    }
    false
}

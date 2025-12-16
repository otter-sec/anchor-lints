use rustc_hir::def_id::DefId;
use rustc_lint::LateContext;
use rustc_middle::mir::Operand;
use rustc_span::{Symbol, source_map::Spanned};

use crate::{mir_analyzer::MirAnalyzer, models::*};

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

// check if the CPI call is new_with_signer
pub fn check_cpi_call_is_new_with_signer<'tcx>(
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
    args: &[Spanned<Operand<'tcx>>],
    fn_def_id: DefId,
) -> bool {
    if let Some(fn_name) = mir_analyzer.cx.tcx.opt_item_name(fn_def_id) {
        let fn_name_str = fn_name.to_string();
        return fn_name_str == "new_with_signer" && args.len() >= 3;
    }
    false
}

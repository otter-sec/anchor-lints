use rustc_lint::LateContext;
use rustc_span::Symbol;

use crate::models::*;

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
                delim_args.tokens.iter().for_each(|token| {
                    if let rustc_ast::tokenstream::TokenTree::Token(token, _) = token {
                        match token.kind {
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
                        }
                    }
                });
            }
        }
    }

    account_constraints
}

/// Check if a field has a specific account constraint (e.g., `init`, `init_if_needed`, `associated_token`).
pub fn has_account_constraint<'tcx>(
    cx: &LateContext<'tcx>,
    field: &rustc_middle::ty::FieldDef,
    constraint_name: &str,
) -> bool {
    let constraint_symbol = Symbol::intern(constraint_name);
    let attrs = cx.tcx.get_all_attrs(field.did);
    for attr in attrs {
        if let rustc_hir::Attribute::Unparsed(_) = attr {
            let item = attr.get_normal_item();
            if let rustc_hir::AttrArgs::Delimited(args) = &item.args {
                for token in args.tokens.iter() {
                    if let rustc_ast::tokenstream::TokenTree::Token(tok, _) = token
                        && let rustc_ast::token::TokenKind::Ident(ident, ..) = tok.kind
                        && ident == constraint_symbol
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

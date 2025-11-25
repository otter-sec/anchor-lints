use rustc_ast::tokenstream::TokenStream;
use rustc_hir::{BinOpKind, Expr, ExprKind, Path as HirPath, QPath, UnOp, def_id::DefId};
use rustc_lint::LateContext;
use rustc_middle::ty::{Ty, TyKind};
use rustc_span::Symbol;
use std::collections::{BTreeSet, HashSet};

use crate::models::*;

pub fn path_to_string(path: &HirPath<'_>) -> String {
    path.segments
        .iter()
        .map(|seg| seg.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

pub fn check_and_add_account_comparison(left: &Expr<'_>, right: &Expr<'_>) -> Vec<String> {
    let mut conditional_account_comparisons: Vec<String> = Vec::new();
    if let (Some(left_account), Some(right_account)) = (
        get_account_name_from_expr(left),
        get_account_name_from_expr(right),
    ) {
        conditional_account_comparisons.push(format!("{}:{}", left_account, right_account));
        conditional_account_comparisons.push(format!("{}:{}", right_account, left_account));
    }
    conditional_account_comparisons
}

fn get_account_name_from_expr(expr: &Expr<'_>) -> Option<String> {
    match expr.kind {
        ExprKind::MethodCall(path_seg, recv, _, _) => {
            if path_seg.ident.name.as_str() == "key"
                && let Some(chain) = extract_field_chain(recv)
                && chain.len() == 3
                && chain[1] == "accounts"
            {
                return Some(chain[2].clone());
            }
        }
        ExprKind::Unary(_, inner_expr) => {
            return get_account_name_from_expr(inner_expr);
        }
        _ => {}
    }
    None
}

pub fn extract_field_chain(expr: &Expr<'_>) -> Option<Vec<String>> {
    let mut chain = vec![];
    let mut current = expr;

    loop {
        match &current.kind {
            ExprKind::Field(base, field) => {
                chain.push(field.to_string());
                current = base;
            }
            ExprKind::Path(qpath) => {
                if let QPath::Resolved(_, path) = qpath {
                    chain.push(path_to_string(path));
                }
                break;
            }
            ExprKind::MethodCall(_, recv, _, _) => {
                current = recv;
            }
            _ => {
                break;
            }
        }
    }

    chain.reverse();
    Some(chain)
}

pub fn get_accounts_def_from_context<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> Option<DefId> {
    if let rustc_middle::ty::Adt(def, substs) = ty.kind() {
        let struct_name = cx.tcx.def_path_str(def.did());
        if (struct_name.ends_with("anchor_lang::context::Context")
            || struct_name.ends_with("anchor_lang::prelude::Context"))
            && let Some(accounts_ty) = substs.types().next()
            && let rustc_middle::ty::Adt(accounts_def, _) = accounts_ty.kind()
        {
            return Some(accounts_def.did());
        }
    }
    None
}

pub fn extract_comparisons<'a>(expr: &'a Expr<'a>) -> Vec<(&'a Expr<'a>, &'a Expr<'a>)> {
    let mut comparisons = Vec::new();
    match &expr.kind {
        ExprKind::Binary(op, left, right) => {
            if BinOpKind::Eq == op.node {
                comparisons.push((*left, *right));
            } else if BinOpKind::Or == op.node {
                let mut left_comparisons = extract_comparisons(left);
                let mut right_comparisons = extract_comparisons(right);
                comparisons.append(&mut left_comparisons);
                comparisons.append(&mut right_comparisons);
            }
        }
        ExprKind::Unary(UnOp::Not, inner_expr) => {
            if let ExprKind::Binary(op, left, right) = &inner_expr.kind
                && BinOpKind::Ne == op.node
            {
                comparisons.push((*left, *right));
            }
            let mut inner_comparisons = extract_inequality_comparisons(inner_expr);
            comparisons.append(&mut inner_comparisons);
        }
        _ => {}
    }

    comparisons
}

pub fn extract_inequality_comparisons<'a>(expr: &'a Expr<'a>) -> Vec<(&'a Expr<'a>, &'a Expr<'a>)> {
    let mut comparisons = Vec::new();

    if let ExprKind::Binary(op, left, right) = &expr.kind {
        if BinOpKind::Ne == op.node {
            comparisons.push((*left, *right));
        } else if BinOpKind::And == op.node {
            let mut left_comparisons = extract_inequality_comparisons(left);
            let mut right_comparisons = extract_inequality_comparisons(right);
            comparisons.append(&mut left_comparisons);
            comparisons.append(&mut right_comparisons);
        }
    }

    comparisons
}

/// Check if two sets of constraints match
pub fn constraints_match(constraints_a: &Vec<String>, constraints_b: &Vec<String>) -> bool {
    let set_a: BTreeSet<_> = constraints_a.iter().map(|s| s.trim()).collect();
    let set_b: BTreeSet<_> = constraints_b.iter().map(|s| s.trim()).collect();
    if set_a != set_b {
        return false;
    }
    true
}

/// Unwrap Box<T> to get T, handling nested Boxes recursively.
pub fn unwrap_box_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> Ty<'tcx> {
    if let TyKind::Adt(adt_def, substs) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        if def_path == "alloc::boxed::Box" || def_path == "std::boxed::Box" {
            let inner = substs.type_at(0);
            // Recursively unwrap nested boxed types
            return unwrap_box_type(cx, inner);
        }
    }
    ty
}

/// Parse a constraint string like "user_a.key!=user_b.key" to extract account names
pub fn parse_constraint_string(constraint: &str) -> Option<(String, String)> {
    let constraint = constraint.trim();

    // Find the != operator
    if let Some(neq_pos) = constraint.find("!=") {
        let left = &constraint[..neq_pos].trim();
        let right = &constraint[neq_pos + 2..].trim();

        let acc1 = left.split('.').next()?.trim();

        let acc2 = right.split('.').next()?.trim();

        if !acc1.is_empty() && !acc2.is_empty() {
            return Some((acc1.to_string(), acc2.to_string()));
        }
    }

    None
}

/// Extract PDA + account constraints from an anchor accounts struct.
/// Return a vector of has_one constraint accounts
pub fn extract_account_constraints<'tcx>(
    cx: &LateContext<'tcx>,
    account_field: &rustc_middle::ty::FieldDef,
    has_one_constraint_accounts: &mut Vec<String>,
) -> AccountConstraint {
    let mut account_constraints: AccountConstraint = AccountConstraint::new();
    let tcx = cx.tcx;
    let attrs = tcx.get_all_attrs(account_field.did);
    let mut last_ident_seeds: bool = false;
    let mut last_ident_constraint: bool = false;
    let mut latest_account_attribute = String::new();
    let mut latest_account_constraint = String::new();

    let mut constraints: Vec<String> = Vec::new();
    for attr in attrs {
        match attr {
            rustc_hir::Attribute::Parsed(_) => {
                // Anchor's #[account(...)] attributes are Unparsed
                // but kept for completeness
            }
            rustc_hir::Attribute::Unparsed(_) => {
                let attr_item = attr.get_normal_item();
                if let rustc_hir::AttrArgs::Delimited(delim_args) = &attr_item.args {
                    delim_args.tokens.iter().for_each(|token| match token {
                        rustc_ast::tokenstream::TokenTree::Token(token, _) => match token.kind {
                            rustc_ast::token::TokenKind::Ident(ident, ..) => {
                                if ident == Symbol::intern("mut") {
                                    account_constraints.mutable = true;
                                } else if ident == Symbol::intern("seeds") {
                                    last_ident_seeds = true;
                                } else if ident == Symbol::intern("constraint") {
                                    last_ident_constraint = true;
                                } else if last_ident_constraint {
                                    latest_account_constraint =
                                        latest_account_constraint.clone() + &ident.to_string();
                                } else {
                                    latest_account_attribute =
                                        latest_account_attribute.clone() + &ident.to_string();
                                }
                            }
                            rustc_ast::token::TokenKind::Comma => {
                                if last_ident_constraint {
                                    last_ident_constraint = false;

                                    if !latest_account_constraint.is_empty() {
                                        constraints.push(latest_account_constraint.clone());
                                        latest_account_constraint = String::new();
                                    }
                                } else if last_ident_seeds {
                                    last_ident_seeds = false;
                                }
                                if !latest_account_attribute.is_empty() {
                                    if latest_account_attribute != "bump" {
                                        check_has_one_constraint(
                                            &latest_account_attribute,
                                            has_one_constraint_accounts,
                                        );

                                        account_constraints
                                            .attributes
                                            .push(latest_account_attribute.clone());
                                    }
                                    latest_account_attribute = String::new();
                                }
                            }

                            rustc_ast::token::TokenKind::Dot => {
                                push_if(last_ident_constraint, &mut latest_account_constraint, ".");
                            }
                            rustc_ast::token::TokenKind::Ne => {
                                push_if(
                                    last_ident_constraint,
                                    &mut latest_account_constraint,
                                    "!=",
                                );
                            }
                            rustc_ast::token::TokenKind::Eq => {
                                push_if(
                                    !latest_account_attribute.is_empty(),
                                    &mut latest_account_attribute,
                                    "=",
                                );
                            }
                            rustc_ast::token::TokenKind::At => {
                                push_if(
                                    !latest_account_attribute.is_empty(),
                                    &mut latest_account_attribute,
                                    "@",
                                );
                            }
                            rustc_ast::token::TokenKind::PathSep => {
                                push_if(
                                    !latest_account_attribute.is_empty(),
                                    &mut latest_account_attribute,
                                    "::",
                                );
                            }
                            _ => {
                                if last_ident_seeds {
                                    last_ident_seeds = false;
                                }
                                if last_ident_constraint {
                                    last_ident_constraint = false;
                                }
                            }
                        },
                        rustc_ast::tokenstream::TokenTree::Delimited(_, _, _, token_stream) => {
                            account_constraints
                                .seeds
                                .extend(recursively_extract_seeds(token_stream, last_ident_seeds));
                        }
                    });
                }
            }
        };
        if !latest_account_attribute.is_empty() && latest_account_attribute != "bump" {
            check_has_one_constraint(&latest_account_attribute, has_one_constraint_accounts);
            account_constraints
                .attributes
                .push(latest_account_attribute.clone());
        }

        if !latest_account_constraint.is_empty() {
            constraints.push(latest_account_constraint.clone());
        }
    }
    if !constraints.is_empty() {
        for constraint_str in &constraints {
            // Parse "user_a.key!=user_b.key" to extract account names
            if let Some((acc1, acc2)) = parse_constraint_string(constraint_str) {
                account_constraints
                    .constraints
                    .push(format!("{}:{}", acc1, acc2));
                account_constraints
                    .constraints
                    .push(format!("{}:{}", acc2, acc1));
            }
        }
    }
    account_constraints
}

/// Extract PDA seeds from a token stream
pub fn recursively_extract_seeds(
    token_stream: &TokenStream,
    last_ident_seeds: bool,
) -> Vec<String> {
    let mut seeds = Vec::new();
    token_stream
        .iter()
        .for_each(|delimited_token_tree| match delimited_token_tree {
            rustc_ast::tokenstream::TokenTree::Token(delimited_token, _) => {
                match delimited_token.kind {
                    rustc_ast::token::TokenKind::Ident(ident, ..) => {
                        if last_ident_seeds {
                            seeds.push(ident.to_string());
                        }
                    }
                    rustc_ast::token::TokenKind::Dot => {
                        if last_ident_seeds {
                            seeds.push(".".to_string());
                        }
                    }
                    rustc_ast::token::TokenKind::Literal(literal, ..) => {
                        if last_ident_seeds {
                            match &literal.kind {
                                rustc_ast::token::LitKind::ByteStr => {
                                    seeds.push(literal.symbol.to_string());
                                }
                                rustc_ast::token::LitKind::Str => {
                                    seeds.push(literal.symbol.to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            rustc_ast::tokenstream::TokenTree::Delimited(_, _, _, token_stream) => {
                let nested_seeds = recursively_extract_seeds(token_stream, last_ident_seeds);
                seeds.push(nested_seeds.join(""));
            }
        });
    seeds
}

pub fn should_report_duplicate(
    struct_id: DefId,
    first: &AccountDetails,
    second: &AccountDetails,
    reported_pairs: &mut HashSet<(DefId, String, String)>,
    conditional: &Vec<String>,
    has_one_constraint_accounts: &Vec<String>,
) -> bool {
    // Skip if one of the accounts is included in the has_one constraint accounts
    if has_one_constraint_accounts.contains(&first.account_name)
        || has_one_constraint_accounts.contains(&second.account_name)
    {
        return false;
    }
    // Deduplicate per struct
    let canonical_pair = (
        struct_id,
        first.account_name.clone(),
        second.account_name.clone(),
    );
    if !reported_pairs.insert(canonical_pair) {
        return false;
    }

    // Attributes must match (e.g. token::mint bindings)
    if !constraints_match(&first.attributes, &second.attributes) {
        return false;
    }

    // Seeds must match (unless one side has none)
    if (!first.seeds.is_empty() || !second.seeds.is_empty()) && first.seeds != second.seeds {
        return false;
    }

    // Skip if there’s already an explicit constraint in the context
    let key = format!("{}:{}", first.account_name, second.account_name);
    let reverse = format!("{}:{}", second.account_name, first.account_name);
    if conditional.contains(&key) || conditional.contains(&reverse) {
        return false;
    }

    true
}

pub fn is_anchor_mutable_account(account_path: &str, constraints: &AccountConstraint) -> bool {
    let is_supported = account_path.starts_with("anchor_lang::prelude::Account")
        || account_path.starts_with("anchor_lang::prelude::InterfaceAccount");

    if !is_supported {
        return false;
    }

    // Account<'info, T> always mutable; InterfaceAccount only when #[account(mut)]
    if account_path.starts_with("anchor_lang::prelude::Account") {
        true
    } else {
        constraints.mutable
    }
}

/// Extract the account name from a has_one constraint
fn check_has_one_constraint(constraint: &str, has_one_constraint_accounts: &mut Vec<String>) {
    if let Some(rest) = constraint.split_once("has_one=") {
        let account = rest.1.split('@').next().unwrap().trim().to_string();

        if !account.is_empty() {
            has_one_constraint_accounts.push(account);
        }
    }
}

fn push_if(check_if: bool, target: &mut String, value: &str) {
    if check_if {
        target.push_str(value);
    }
}

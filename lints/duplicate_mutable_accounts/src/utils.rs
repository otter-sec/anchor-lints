use regex::Regex;
use rustc_hir::{BinOpKind, Expr, ExprKind, Path as HirPath, QPath, UnOp, def_id::DefId};
use rustc_lint::LateContext;
use rustc_middle::ty::{AdtDef, Ty};

use clippy_utils::source::HasSession;

pub fn get_struct_body(lines: &Vec<&str>, start_line_idx: usize) -> String {
    let mut struct_body = String::new();
    let mut brace_count = 0;
    let mut started = false;

    for (_, line) in lines.iter().enumerate().skip(start_line_idx) {
        if !started {
            if line.contains('{') {
                started = true;
                brace_count += 1;
            }
            struct_body.push_str(line);
            struct_body.push('\n');
        } else {
            brace_count += line.matches('{').count();
            brace_count -= line.matches('}').count();

            struct_body.push_str(line);
            struct_body.push('\n');

            if brace_count == 0 {
                break;
            }
        }
    }
    struct_body
}
pub fn parse_constraints_from_source(source: &str) -> Vec<String> {
    let mut accounts: Vec<String> = Vec::new();
    let mut search_start = 0;

    loop {
        // look for "#[account" patterns
        if let Some(account_attr_start) = source[search_start..].find("#[account") {
            let attr_start = search_start + account_attr_start;
            // skip comments
            if source[..attr_start].trim_end().ends_with("//") {
                search_start = attr_start + 1;
                continue;
            }
            let constraint_re = Regex::new(r"(\w+)\.key\(\)\s*!=\s*(\w+)\.key\(\)").unwrap();

            if let Some(attr_end) = find_closing_parenthesis(&source[attr_start..]) {
                let attr_text = &source[attr_start..attr_start + attr_end];
                if attr_text.contains("constraint") {
                    // match expression: account_a.key() != account_b.key()
                    for cap in constraint_re.captures_iter(attr_text) {
                        accounts.push(format!("{}:{}", cap[1].to_string(), cap[2].to_string()));
                        accounts.push(format!("{}:{}", cap[2].to_string(), cap[1].to_string()));
                    }
                }
                search_start = attr_start + attr_end;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    accounts
}

pub fn find_closing_parenthesis(text: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in text.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match ch {
            '\\' => escape_next = true,
            '"' | '\'' => in_string = !in_string,
            '(' if !in_string => depth += 1,
            ')' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn path_to_string(path: &HirPath<'_>) -> String {
    path.segments
        .iter()
        .map(|seg| seg.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

pub fn check_and_add_account_comparison<'tcx>(
    cx: &LateContext<'tcx>,
    left: &Expr<'_>,
    right: &Expr<'_>,
) -> Vec<String> {
    let mut conditional_account_comparisons: Vec<String> = Vec::new();
    if let (Some(left_account), Some(right_account)) = (
        get_account_name_from_expr(cx, left),
        get_account_name_from_expr(cx, right),
    ) {
        conditional_account_comparisons.push(format!("{}:{}", left_account, right_account));
        conditional_account_comparisons.push(format!("{}:{}", right_account, left_account));
    }
    conditional_account_comparisons
}

fn get_account_name_from_expr<'tcx>(cx: &LateContext<'tcx>, expr: &Expr<'_>) -> Option<String> {
    match expr.kind {
        ExprKind::MethodCall(path_seg, recv, _, _) => {
            if path_seg.ident.name.as_str() == "key" {
                if let Some(chain) = extract_field_chain(recv) {
                    if chain.len() == 3 && chain[1] == "accounts" {
                        return Some(chain[2].clone());
                    }
                }
            }
        }
        ExprKind::Unary(_, inner_expr) => {
            return get_account_name_from_expr(cx, inner_expr);
        }
        _ => {}
    }
    None
}

fn extract_field_chain(expr: &Expr<'_>) -> Option<Vec<String>> {
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
        if struct_name.ends_with("anchor_lang::context::Context")
            || struct_name.ends_with("anchor_lang::prelude::Context")
        {
            if let Some(accounts_ty) = substs.types().next() {
                if let rustc_middle::ty::Adt(accounts_def, _) = accounts_ty.kind() {
                    return Some(accounts_def.did());
                }
            }
        }
    }
    None
}

pub fn extract_account_constraints_from_struct<'tcx>(
    cx: &LateContext<'tcx>,
    adt_def: &AdtDef,
) -> Vec<String> {
    let mut constraint_accounts: Vec<String> = Vec::new();

    let struct_def_id = adt_def.did();
    let struct_span = cx.tcx.def_span(struct_def_id);
    let source_map = cx.sess().source_map();

    // get struct body from source code
    if let Ok(file_span) = source_map.span_to_lines(struct_span) {
        let file = &file_span.file;
        let start_line_idx = file_span.lines[0].line_index;
        if let Some(src) = file.src.as_ref() {
            let lines: Vec<&str> = src.lines().collect();
            let struct_body = get_struct_body(&lines, start_line_idx);
            constraint_accounts = parse_constraints_from_source(&struct_body);
        }
    }

    constraint_accounts
}

pub fn extract_comparisons<'a>(expr: &'a Expr<'a>) -> Vec<(&'a Expr<'a>, &'a Expr<'a>)> {
    let mut comparisons = Vec::new();
    match &expr.kind {
        ExprKind::Binary(op, left, right) => {
            if BinOpKind::Eq == op.node {
                comparisons.push((*left, *right));
            } else if BinOpKind::Or == op.node {
                let mut left_comparisons = extract_comparisons(*left);
                let mut right_comparisons = extract_comparisons(*right);
                comparisons.append(&mut left_comparisons);
                comparisons.append(&mut right_comparisons);
            }
        }
        ExprKind::Unary(UnOp::Not, inner_expr) => {
            if let ExprKind::Binary(op, left, right) = &inner_expr.kind {
                if BinOpKind::Ne == op.node {
                    comparisons.push((*left, *right));
                }
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

    match &expr.kind {
        ExprKind::Binary(op, left, right) => {
            if BinOpKind::Ne == op.node {
                comparisons.push((*left, *right));
            } else if BinOpKind::And == op.node {
                let mut left_comparisons = extract_inequality_comparisons(*left);
                let mut right_comparisons = extract_inequality_comparisons(*right);
                comparisons.append(&mut left_comparisons);
                comparisons.append(&mut right_comparisons);
            }
        }
        _ => {}
    }

    comparisons
}

#![feature(rustc_private)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use rustc_hir::{
    Body as HirBody, Expr, ExprKind, FnDecl,
    def_id::LocalDefId,
    intravisit::{FnKind, Visitor, walk_expr},
};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty::{Ty, TyKind};
use rustc_span::Span;

use clippy_utils::{
    diagnostics::span_lint_and_help, fn_has_unsatisfiable_preds, source::HasSession,
};
use std::collections::HashMap;

mod utils;
use utils::*;

dylint_linting::declare_late_lint! {
    /// ### What it does
    /// Detects duplicate mutable account usage in functions,
    /// where the same account is passed into multiple mutable parameters.
    ///
    /// ### Why is this bad?
    /// This can lead to unexpected aliasing of mutable data, logical errors, and vulnerabilities like
    /// account state corruption.
    ///
    pub DUPLICATE_MUTABLE_ACCOUNTS,
    Warn,
    "detect duplicate mutable accounts"
}
#[derive(Debug, Clone)]
struct AccountDetails {
    span: Span,
    account_name: String,
}

#[derive(Debug)]
struct DuplicateContextAccounts {
    accounts: Vec<AccountDetails>,
}

impl<'tcx> LateLintPass<'tcx> for DuplicateMutableAccounts {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        _kind: FnKind<'tcx>,
        _: &FnDecl<'tcx>,
        body: &HirBody<'tcx>,
        fn_span: Span,
        def_id: LocalDefId,
    ) {
        // skip macro expansions
        if fn_span.from_expansion() {
            return;
        }
        // skip functions with unsatisfiable predicates
        if fn_has_unsatisfiable_preds(cx, def_id.to_def_id()) {
            return;
        }

        let mut mutable_accounts: HashMap<Ty, DuplicateContextAccounts> = HashMap::new();
        let mut conditional_account_comparisons: Vec<String> = Vec::new();

        // check function's first argument which is the context type
        let params = &body.params;
        let ctx_param = &params[0].pat;
        let ctx_ty = cx.typeck_results().pat_ty(ctx_param);

        // Find duplicate accounts in anchor context's accounts field with same type
        // Check for account key comparisons (via Anchor constraints or manual key() comparisons)
        // If duplicate accounts, without any comparisons found report lint

        // If argument is context type then get account struct def id
        if let Some(accounts_struct_def_id) = get_accounts_def_from_context(cx, ctx_ty) {
            let accounts_struct_span = cx.tcx.def_span(accounts_struct_def_id);
            if let TyKind::Adt(adt_def, generics) = ctx_ty.kind() {
                // extract context struct fields
                let variant = adt_def.non_enum_variant();
                for field in &variant.fields {
                    let field_name = field.ident(cx.tcx).to_string();
                    let field_ty = field.ty(cx.tcx, generics);
                    if field_name == "accounts" {
                        let accounts_struct_ty = field_ty.peel_refs();
                        if let TyKind::Adt(accounts_adt_def, accounts_generics) =
                            accounts_struct_ty.kind()
                        {
                            // add anchor constraints
                            conditional_account_comparisons.extend(
                                extract_account_constraints_from_struct(cx, accounts_adt_def),
                            );
                            let accounts_variant = accounts_adt_def.non_enum_variant();
                            for account_field in &accounts_variant.fields {
                                let account_name = account_field.ident(cx.tcx).to_string();
                                let account_span = cx.tcx.def_span(account_field.did);
                                let account_ty = account_field.ty(cx.tcx, accounts_generics);
                                if let TyKind::Adt(adt_def, _) = account_ty.kind() {
                                    let account_path = cx.tcx.def_path_str(adt_def.did());
                                    if account_path.starts_with("anchor_lang::prelude::Account") {
                                        let existing_accounts = mutable_accounts
                                            .get(&account_ty)
                                            .map(|d| d.accounts.clone())
                                            .unwrap_or_default();
                                        mutable_accounts.insert(
                                            account_ty,
                                            DuplicateContextAccounts {
                                                accounts: {
                                                    let mut accounts = existing_accounts;
                                                    accounts.push(AccountDetails {
                                                        span: account_span,
                                                        account_name,
                                                    });
                                                    accounts
                                                },
                                            },
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // add manual account key checks
            conditional_account_comparisons
                .extend(check_manual_account_comparisons(cx, body.value));

            for duplicate_context_accounts in mutable_accounts.values() {
                let accounts = &duplicate_context_accounts.accounts;
                let account_count = accounts.len();

                if account_count > 1 {
                    for i in 0..account_count {
                        for j in i + 1..account_count {
                            let first = &accounts[i];
                            let second = &accounts[j];
                            let accounts_key =
                                format!("{}:{}", first.account_name, second.account_name);

                            if !conditional_account_comparisons.contains(&accounts_key)
                                && !conditional_account_comparisons.contains(&format!(
                                    "{}:{}",
                                    second.account_name, first.account_name
                                ))
                            {
                                let help_message = format!(
                                    "`{}` and `{}` may refer to the same account. \
                                    Consider adding a constraint like `#[account(constraint = {}.key() != {}.key())]`.",
                                    first.account_name,
                                    second.account_name,
                                    first.account_name,
                                    second.account_name,
                                );

                                span_lint_and_help(
                                    cx,
                                    DUPLICATE_MUTABLE_ACCOUNTS,
                                    accounts_struct_span,
                                    "duplicate mutable account found",
                                    Some(first.span),
                                    help_message,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

fn check_manual_account_comparisons<'tcx>(
    cx: &LateContext<'tcx>,
    expr: &'tcx Expr<'tcx>,
) -> Vec<String> {
    struct ExprVisitor<'a, 'tcx> {
        cx: &'a LateContext<'tcx>,
        conditional_account_comparisons: Vec<String>,
    }

    impl<'a, 'tcx> Visitor<'tcx> for ExprVisitor<'a, 'tcx> {
        fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
            // if expression
            if let ExprKind::If(cond, then_block, _) = &expr.kind {
                let has_exit = contains_exit_statement(then_block, self.cx);

                if has_exit {
                    // extracting all comparisons from the condition
                    let comparisons = extract_comparisons(cond);
                    for (left, right) in comparisons {
                        self.conditional_account_comparisons
                            .extend(check_and_add_account_comparison(left, right));
                    }
                }
            }

            walk_expr(self, expr);
        }
    }

    let mut visitor = ExprVisitor {
        cx,
        conditional_account_comparisons: Vec::new(),
    };
    visitor.visit_expr(expr);

    visitor.conditional_account_comparisons
}

fn contains_exit_statement<'tcx>(expr: &'tcx Expr<'tcx>, cx: &LateContext<'tcx>) -> bool {
    struct ExitFinder<'a, 'tcx> {
        cx: &'a LateContext<'tcx>,
        found: bool,
    }

    impl<'a, 'tcx> Visitor<'tcx> for ExitFinder<'a, 'tcx> {
        fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
            if self.found {
                return;
            }

            // check for return statement
            if let ExprKind::Ret(_) = expr.kind {
                self.found = true;
                return;
            }

            // check for panic! macro in source code
            if expr.span.from_expansion() {
                let source_span = expr.span.source_callsite();
                if let Ok(source_text) = self.cx.sess().source_map().span_to_snippet(source_span)
                    && source_text.trim_start().starts_with("panic!")
                {
                    self.found = true;
                    return;
                }
            }

            walk_expr(self, expr);
        }
    }

    let mut finder = ExitFinder { cx, found: false };
    walk_expr(&mut finder, expr);
    finder.found
}

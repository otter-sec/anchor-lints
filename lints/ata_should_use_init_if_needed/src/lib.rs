#![feature(rustc_private)]
#![warn(unused_extern_crates)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use anchor_lints_utils::mir_analyzer::MirAnalyzer;

use clippy_utils::{diagnostics::span_lint, fn_has_unsatisfiable_preds};
use rustc_hir::{Body as HirBody, FnDecl, def_id::LocalDefId, intravisit::FnKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty::{Ty, TyKind};
use rustc_span::Span;

use anchor_lints_utils::utils::account_constraints::has_account_constraint;

dylint_linting::declare_late_lint! {
    /// ### What it does
    /// Detects Associated Token Accounts (ATAs) that use `init` constraint instead of `init_if_needed`.
    ///
    /// ### Why is this bad?
    /// Using `init` on an ATA will fail if the account already exists. `init_if_needed` will only
    /// initialize the account if it doesn't exist, making the instruction idempotent and preventing
    /// transaction failures when the ATA already exists.
    ///
    /// ### Bad
    /// ```rust
    /// #[derive(Accounts)]
    /// pub struct Deposit<'info> {
    ///     #[account(
    ///         init,  // Will fail if ATA already exists
    ///         associated_token::authority = user,
    ///         associated_token::mint = mint,
    ///         associated_token::token_program = token_program,
    ///         payer = user
    ///     )]
    ///     pub user_token_account: Account<'info, TokenAccount>,
    ///     // ...
    /// }
    /// ```
    ///
    /// ### Good
    /// ```rust
    /// #[derive(Accounts)]
    /// pub struct Deposit<'info> {
    ///     #[account(
    ///         init_if_needed,  // Works whether ATA exists or not
    ///         associated_token::authority = user,
    ///         associated_token::mint = mint,
    ///         associated_token::token_program = token_program,
    ///         payer = user
    ///     )]
    ///     pub user_token_account: Account<'info, TokenAccount>,
    ///     // ...
    /// }
    /// ```
    pub ATA_SHOULD_USE_INIT_IF_NEEDED,
    Warn,
    "Associated Token Account uses 'init' instead of 'init_if_needed'"
}

impl<'tcx> LateLintPass<'tcx> for AtaShouldUseInitIfNeeded {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        _kind: FnKind<'tcx>,
        _: &'tcx FnDecl<'tcx>,
        body: &'tcx HirBody<'tcx>,
        fn_span: Span,
        def_id: LocalDefId,
    ) {
        // Skip macro expansions
        if fn_span.from_expansion() {
            return;
        }

        // Skip functions with unsatisfiable predicates
        if fn_has_unsatisfiable_preds(cx, def_id.to_def_id()) {
            return;
        }

        analyze_ata_init_constraint(cx, body, def_id);
    }
}

fn analyze_ata_init_constraint<'tcx>(
    cx: &LateContext<'tcx>,
    body: &'tcx HirBody<'tcx>,
    def_id: LocalDefId,
) {
    let mir_analyzer = MirAnalyzer::new(cx, body, def_id);

    // Analyze functions that take Anchor context
    let Some(anchor_context) = mir_analyzer.anchor_context_info.as_ref() else {
        return;
    };

    // Extract accounts with init constraint and associated_token constraints
    let accounts_with_issue = extract_ata_with_init_constraint(cx, anchor_context);

    for (account_name, account_span) in accounts_with_issue {
        span_lint(
            cx,
            ATA_SHOULD_USE_INIT_IF_NEEDED,
            account_span,
            format!(
                "Associated Token Account `{}` uses `init` constraint. Consider using `init_if_needed` instead to make the instruction idempotent.",
                account_name
            ),
        );
    }
}

/// Extract accounts that have both `init` constraint and `associated_token` constraints
fn extract_ata_with_init_constraint<'tcx>(
    cx: &LateContext<'tcx>,
    anchor_context: &anchor_lints_utils::mir_analyzer::AnchorContextInfo<'tcx>,
) -> Vec<(String, Span)> {
    let mut result = Vec::new();
    let accounts_struct_ty = &anchor_context.anchor_context_account_type;

    if let TyKind::Adt(adt_def, generics) = accounts_struct_ty.kind() {
        if !adt_def.is_struct() && !adt_def.is_union() {
            return result;
        }
        let variant = adt_def.non_enum_variant();
        for field in &variant.fields {
            let account_name = field.ident(cx.tcx).to_string();
            let account_span = cx.tcx.def_span(field.did);
            let account_ty = field.ty(cx.tcx, generics);

            // Check if field has `init` constraint (not `init_if_needed`)
            if !has_account_constraint(cx, field, "init") {
                continue;
            }

            // Check if field has `init_if_needed` - if so, skip (this is the correct pattern)
            if has_account_constraint(cx, field, "init_if_needed") {
                continue;
            }

            // Check if field has `associated_token` constraints
            if !has_account_constraint(cx, field, "associated_token") {
                continue;
            }

            // Check if field type is TokenAccount or InterfaceAccount<'info, TokenAccount>
            if is_token_account_type(cx, account_ty) {
                result.push((account_name, account_span));
            }
        }
    }

    result
}

/// Check if a type is TokenAccount or InterfaceAccount<'info, TokenAccount>.
fn is_token_account_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, substs) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());

        // Check for Account<'info, TokenAccount> or AccountLoader<'info, TokenAccount>
        if (def_path.contains("anchor_lang::prelude::Account")
            || def_path.ends_with("anchor_lang::accounts::account::Account")
            || def_path.starts_with("anchor_lang::prelude::AccountLoader"))
            && !substs.is_empty()
            && let Some(inner_ty) = substs.types().next()
        {
            return is_token_account_inner_type(cx, inner_ty);
        }
        // Check for InterfaceAccount<'info, TokenAccount>
        if (def_path.contains("anchor_lang::prelude::InterfaceAccount")
            || def_path.contains("anchor_spl::token::InterfaceAccount"))
            && !substs.is_empty()
            && let Some(inner_ty) = substs.types().next()
        {
            return is_token_account_inner_type(cx, inner_ty);
        }
    }
    false
}

/// Check if the inner type is TokenAccount.
fn is_token_account_inner_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, _) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());

        // Check for various TokenAccount types
        if def_path.contains("anchor_spl::token::TokenAccount")
            || def_path.contains("anchor_spl::token_interface::TokenAccount")
            || def_path.contains("spl_token::state::Account")
        {
            return true;
        }
    }
    false
}

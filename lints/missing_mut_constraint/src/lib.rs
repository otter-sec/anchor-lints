#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use anchor_lints_utils::{
    mir_analyzer::{AnchorContextInfo, MirAnalyzer},
    utils::{extract_account_constraints, should_skip_function},
};
use clippy_utils::diagnostics::span_lint;

use rustc_hir::{Body as HirBody, FnDecl, def_id::LocalDefId, intravisit::FnKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::{
    mir::{Mutability, Place, Rvalue, StatementKind},
    ty::TyKind,
};
use rustc_span::Span;

use std::collections::{HashMap, HashSet};

dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Detects when an account is mutated in the instruction body but not declared
    /// with `#[account(mut)]` in the Anchor accounts struct.
    ///
    /// ### Why is this bad?
    /// Mutating an account without the `mut` constraint can cause the runtime to
    /// reject the transaction or behave unexpectedly, as the account was not
    /// marked as writable.
    ///
    /// ### Example
    /// ```rust
    /// #[derive(Accounts)]
    /// pub struct Update<'info> {
    ///     pub vault: Account<'info, Vault>,  // missing #[account(mut)]
    /// }
    /// pub fn update(ctx: Context<Update>) -> Result<()> {
    ///     ctx.accounts.vault.amount += 1;  // mutation
    ///     Ok(())
    /// }
    /// ```
    ///
    /// ### Good
    /// ```rust
    /// #[derive(Accounts)]
    /// pub struct Update<'info> {
    ///     #[account(mut)]
    ///     pub vault: Account<'info, Vault>,
    /// }
    /// ```
    pub MISSING_MUT_CONSTRAINT,
    Warn,
    "account is mutated but missing #[account(mut)]",
    MissingMutConstraint
}

#[derive(Default)]
pub struct MissingMutConstraint;

struct AccountMutability {
    span: Span,
    mutable: bool,
}

impl<'tcx> LateLintPass<'tcx> for MissingMutConstraint {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        _kind: FnKind<'tcx>,
        _: &FnDecl<'tcx>,
        body: &HirBody<'tcx>,
        main_fn_span: Span,
        def_id: LocalDefId,
    ) {
        if should_skip_function(cx, main_fn_span, def_id) {
            return;
        }

        let mut mir_analyzer = MirAnalyzer::new(cx, body, def_id);
        anchor_lints_utils::utils::ensure_anchor_context_initialized(&mut mir_analyzer, body);

        // Analyze functions that take Anchor context
        let Some(anchor_context_info) = mir_analyzer.anchor_context_info.as_ref() else {
            return;
        };

        analyze_missing_mut_constraint(cx, &mir_analyzer, anchor_context_info);
    }
}

fn analyze_missing_mut_constraint<'cx, 'tcx>(
    cx: &'cx LateContext<'tcx>,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    anchor_context_info: &AnchorContextInfo<'tcx>,
) {
    let accounts_struct_ty = &anchor_context_info.anchor_context_account_type;
    let TyKind::Adt(adt_def, _generics) = accounts_struct_ty.kind() else {
        return;
    };

    if !adt_def.is_struct() && !adt_def.is_union() {
        return;
    }

    let variant = adt_def.non_enum_variant();
    let mut account_mutability: HashMap<String, AccountMutability> = HashMap::new();

    for field in &variant.fields {
        let account_name = field.ident(cx.tcx).to_string();
        let account_span = cx.tcx.def_span(field.did);
        let constraints = extract_account_constraints(cx, field);
        account_mutability.insert(
            account_name,
            AccountMutability {
                span: account_span,
                mutable: constraints.mutable,
            },
        );
    }

    let mutated_accounts = collect_mutated_accounts(mir_analyzer);
    let mut visited = HashSet::new();

    for account_name in mutated_accounts {
        if visited.contains(&account_name) {
            continue;
        }
        visited.insert(account_name.clone());

        if let Some(info) = account_mutability.get(&account_name)
            && !info.mutable
        {
            span_lint(
                cx,
                MISSING_MUT_CONSTRAINT,
                info.span,
                format!(
                    "account `{}` is mutated in the instruction but is not declared with `#[account(mut)]`",
                    account_name
                ),
            );
        }
    }
}

/// Collects the accounts that are mutated in the instruction body
fn collect_mutated_accounts<'cx, 'tcx>(mir_analyzer: &MirAnalyzer<'cx, 'tcx>) -> HashSet<String> {
    let mut mutated = HashSet::new();

    for (_bb, bbdata) in mir_analyzer.mir.basic_blocks.iter_enumerated() {
        for stmt in &bbdata.statements {
            if let StatementKind::Assign(box (place, rvalue)) = &stmt.kind
                && let Some(account_name) =
                    account_name_from_place_or_rvalue(mir_analyzer, place, rvalue)
            {
                mutated.insert(account_name);
            }
        }
    }

    mutated
}

/// Extracts the account name from a place or rvalue
fn account_name_from_place_or_rvalue<'cx, 'tcx>(
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    place: &Place<'_>,
    rvalue: &Rvalue<'_>,
) -> Option<String> {
    let base_local = place.local;
    let resolved = mir_analyzer.resolve_to_original_local(base_local, &mut HashSet::new());
    if let Some(acc) = mir_analyzer.extract_account_name_from_local(&resolved, true) {
        let name = acc
            .account_name
            .split('.')
            .next()
            .unwrap_or(&acc.account_name)
            .to_string();
        return Some(name);
    }

    if let Rvalue::Ref(_, borrow_kind, ref_place) = rvalue
        && borrow_kind.mutability() == Mutability::Mut
    {
        let base = ref_place.local;
        let resolved = mir_analyzer.resolve_to_original_local(base, &mut HashSet::new());
        if let Some(acc) = mir_analyzer.extract_account_name_from_local(&resolved, true) {
            let name = acc
                .account_name
                .split('.')
                .next()
                .unwrap_or(&acc.account_name)
                .to_string();
            return Some(name);
        }
    }

    None
}

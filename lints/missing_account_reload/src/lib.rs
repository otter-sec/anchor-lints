#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use std::collections::{HashMap, HashSet};

use clippy_utils::{
    diagnostics::span_lint_and_note, fn_has_unsatisfiable_preds, ty::is_type_diagnostic_item,
};

use rustc_hir::{Body as HirBody, FnDecl, def_id::LocalDefId, intravisit::FnKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::{
    mir::{BasicBlock, Operand, TerminatorKind},
    ty::{self as rustc_ty},
};
use rustc_span::{Span, Symbol};

mod models;
mod utils;

use models::*;
use utils::*;

dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Identifies access of an account without calling `reload()` after a CPI.
    ///
    /// ### Why is this bad?
    /// After a CPI, deserialized accounts do not have their data updated automatically.
    /// Accessing them without calling `reload` may lead to stale data being loaded.
    /// ```
    pub MISSING_ACCOUNT_RELOAD,
    Warn,
    "account accessed after a CPI without reloading",
    MissingAccountReload
}

#[derive(Default)]
pub struct MissingAccountReload;

impl<'tcx> LateLintPass<'tcx> for MissingAccountReload {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        _kind: FnKind<'tcx>,
        _: &FnDecl<'tcx>,
        body: &HirBody<'tcx>,
        main_fn_span: Span,
        def_id: LocalDefId,
    ) {
        // skip macro expansions
        if main_fn_span.from_expansion() {
            return;
        }
        // Building MIR for `fn`s with unsatisfiable preds results in ICE.
        if fn_has_unsatisfiable_preds(cx, def_id.to_def_id()) {
            return;
        }

        let fn_crate_name = cx.tcx.crate_name(def_id.to_def_id().krate).to_string();

        let account_reload_sym = Symbol::intern("AnchorAccountReload");

        let deref_method_sym = Symbol::intern("deref_method");
        let cpi_invoke_syms = [
            Symbol::intern("AnchorCpiInvoke"),
            Symbol::intern("AnchorCpiInvokeUnchecked"),
            Symbol::intern("AnchorCpiInvokeSigned"),
            Symbol::intern("AnchorCpiInvokeSignedUnchecked"),
        ];
        let anchor_cpi_sym = Symbol::intern("AnchorCpiContext");

        let mir = cx.tcx.optimized_mir(def_id.to_def_id());

        // If fn does not take a anchor context, skip to avoid false positives
        let Some(anchor_context_info) = get_anchor_context_accounts(cx, body) else {
            return;
        };

        // We need to identify
        // A) CPI invocations
        // Then, for each account
        // B) Account data accesses (i.e. a call to `Deref` on `Account.name`)
        // C) Account reloads (i.e. a call to `Account.name::reload`)
        // We need to identify all (B) which are dominated by (A) and *not* dominated by a corresponding (C)

        // BBs terminated by a CPI
        let mut cpi_calls: HashMap<BasicBlock, Span> = HashMap::new();
        // Map of account fields to BBs accessing them
        let mut account_accesses: HashMap<String, HashMap<BasicBlock, Span>> = HashMap::new();
        // Map of account fields to BBs reloading them
        let mut account_reloads: HashMap<String, HashSet<BasicBlock>> = HashMap::new();
        // Map of CPI context account types
        let mut cpi_accounts: HashMap<String, BasicBlock> = HashMap::new();

        // Map of account names invoked in a CPI & local map of assignments
        let (cpi_accounts_map, reverse_assignment_map) = build_local_relationship_maps(mir);
        let transitive_assignment_reverse_map =
            build_transitive_reverse_map(&reverse_assignment_map);

        for (bb, bbdata) in mir.basic_blocks.iter_enumerated() {
            // Locate blocks ending with a call
            if let TerminatorKind::Call {
                func: Operand::Constant(func),
                args,
                fn_span,
                ..
            } = &bbdata.terminator().kind
                && let rustc_ty::FnDef(fn_def_id, _) = func.ty().kind()
            {
                let crate_name = cx.tcx.crate_name(fn_def_id.krate).to_string();
                let fn_sig = cx.tcx.fn_sig(*fn_def_id).skip_binder();
                let fn_sig_unbounded = fn_sig.skip_binder();
                let return_ty = fn_sig_unbounded.output();
                // Check that it is a diag item
                if let Some(diag_item) = cx
                    .tcx
                    .diagnostic_items(fn_def_id.krate)
                    .id_to_name
                    .get(fn_def_id)
                {
                    // Check if it is Account::reload...
                    if *diag_item == account_reload_sym {
                        // Extract the receiver
                        if let Some(account) = args.first()
                            && let Operand::Move(account) = account.node
                            && let Some(local) = account.as_local()
                            // Check if the local is an account name
                            && let Some(account_name_and_local) = check_local_and_assignment_locals(
                                cx,
                                mir,
                                &local,
                                &transitive_assignment_reverse_map,
                                &mut HashSet::new(),
                                false,
                            )
                        {
                            account_reloads
                                .entry(account_name_and_local.account_name)
                                .or_default()
                                .insert(bb);
                        }
                    }
                    // Or a CPI invoke function
                    else if cpi_invoke_syms.contains(diag_item) {
                        cpi_calls.insert(bb, *fn_span);
                    } else if *diag_item == deref_method_sym
                        && let Some(account) = args.first()
                        && let Operand::Move(account) = account.node
                        && let Some(local) = account.as_local()
                        // Check if the local is an account name
                        && let Some(account_name_and_local) = check_local_and_assignment_locals(
                            cx,
                            mir,
                            &local,
                            &transitive_assignment_reverse_map,
                            &mut HashSet::new(),
                            false,
                        )
                    {
                        account_accesses
                            .entry(account_name_and_local.account_name)
                            .or_default()
                            .insert(bb, *fn_span);
                    }
                } else if takes_cpi_context(cx, mir, args) {
                    cpi_calls.insert(bb, *fn_span);
                }
                // CPI context
                else if is_type_diagnostic_item(cx, return_ty, anchor_cpi_sym) {
                    if let Some(cpi_accounts_struct) = args.get(1)
                        && let Operand::Copy(place) | Operand::Move(place) =
                            &cpi_accounts_struct.node
                        && let Some(accounts_local) = place.as_local()
                        && let Some(accounts) = find_cpi_accounts_struct(
                            &accounts_local,
                            &reverse_assignment_map,
                            &cpi_accounts_map,
                            &mut HashSet::new(),
                        )
                    {
                        for account_local in accounts {
                            // Check if the local is an account name
                            if let Some(account_name_and_local) = check_local_and_assignment_locals(
                                cx,
                                mir,
                                &account_local,
                                &transitive_assignment_reverse_map,
                                &mut HashSet::new(),
                                false,
                            ) {
                                cpi_accounts.insert(account_name_and_local.account_name, bb);
                            }
                        }
                    }
                // Check if the function is a nested function
                } else if crate_name == fn_crate_name
                    // check fn takes context/context.accounts/context.accounts.account as arguments
                    && let Some(nested_argument) =
                        get_nested_fn_arguments(cx, mir, args, &anchor_context_info)
                {
                    // Called fn has reloads for its arguments
                    let fn_account_reloads = check_nested_account_reloads(
                        cx,
                        fn_def_id,
                        &fn_crate_name,
                        &anchor_context_info,
                    );
                    for (account_name, (account_ty, arg_local)) in fn_account_reloads.into_iter() {
                        if nested_argument.arg_type == NestedArgumentType::Account {
                            for (nested_account_name, (nested_account_ty, nested_arg_local)) in
                                nested_argument.accounts.clone().into_iter()
                            {
                                if nested_account_ty == account_ty && arg_local == nested_arg_local
                                {
                                    let reload_account_name = format!(
                                        "{}.accounts.{}",
                                        anchor_context_info.anchor_context_name,
                                        nested_account_name
                                    );
                                    account_reloads
                                        .entry(reload_account_name)
                                        .or_default()
                                        .insert(bb);
                                }
                            }
                        } else {
                            let reload_account_name = format!(
                                "{}.accounts.{}",
                                anchor_context_info.anchor_context_name, account_name
                            );
                            account_reloads
                                .entry(reload_account_name)
                                .or_default()
                                .insert(bb);
                        }
                    }
                }
            }
        }

        let cpi_call_blocks: HashSet<_> = cpi_calls.keys().copied().collect();

        // Filter accounts to only those involved in CPI calls
        cpi_accounts
            .retain(|_ty, &mut block| reachable_blocks(&mir.basic_blocks, block, &cpi_call_blocks));

        // Only check account accesses for accounts used in CPI
        account_accesses.retain(|name, _| cpi_accounts.contains_key(name));

        // For each account access, check if it happens after CPI without a reload
        for (ty, accesses) in account_accesses.into_iter() {
            let access_blocks = accesses.keys().copied().collect();
            let reloads = account_reloads.remove(&ty).unwrap_or_default();
            for (access, cpi) in reachable_without_passing(
                &mir.basic_blocks,
                cpi_call_blocks.clone(),
                access_blocks,
                reloads,
            ) {
                span_lint_and_note(
                    cx,
                    MISSING_ACCOUNT_RELOAD,
                    accesses[&access],
                    "accessing an account after a CPI without calling `reload()`",
                    Some(cpi_calls[&cpi]),
                    "CPI is here",
                );
            }
        }
    }
}

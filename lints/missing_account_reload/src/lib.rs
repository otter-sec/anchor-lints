#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use std::collections::{HashMap, HashSet};

use anchor_lints_utils::{
    diag_items::DiagnoticItem,
    mir_analyzer::{AnchorContextInfo, MirAnalyzer},
};
use clippy_utils::{
    diagnostics::{span_lint, span_lint_and_note},
    fn_has_unsatisfiable_preds,
};

use rustc_hir::{
    Body as HirBody, FnDecl,
    def_id::{DefId, LocalDefId},
    intravisit::FnKind,
};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::{
    mir::{BasicBlock, HasLocalDecls, Local, Operand, TerminatorKind},
    ty::{self as rustc_ty, TyCtxt},
};
use rustc_span::Span;

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

        let mir_analyzer = MirAnalyzer::new(cx, body, def_id);
        let mir = mir_analyzer.mir;
        // If fn does not take a anchor context, skip to avoid false positives
        let Some(anchor_context_info) = mir_analyzer.anchor_context_info.as_ref() else {
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
        let mut account_accesses: HashMap<String, Vec<AccountAccess>> = HashMap::new();
        // Map of account fields to BBs reloading them
        let mut account_reloads: HashMap<String, HashSet<BasicBlock>> = HashMap::new();
        // Map of CPI context account types
        let mut cpi_accounts: HashMap<String, BasicBlock> = HashMap::new();

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
                let return_ty = fn_sig.skip_binder().output();

                // Check if it is Account::reload...
                if DiagnoticItem::AnchorAccountReload.defid_is_item(cx.tcx, *fn_def_id) {
                    // Extract the receiver
                    if let Some(account) = args.first()
                        && let Operand::Move(account) = account.node
                        && let Some(local) = account.as_local()
                    {
                        // Check if the local is an account name
                        if let Some(account_name_and_local) =
                            extract_account_name_from_local(&mir_analyzer, &local, false)
                        {
                            account_reloads
                                .entry(account_name_and_local.account_name)
                                .or_default()
                                .insert(bb);
                        }
                    }
                }
                // Or a CPI invoke function
                else if is_cpi_invoke_fn(cx.tcx, *fn_def_id) || takes_cpi_context(cx, mir, args) {
                    cpi_calls.insert(bb, *fn_span);
                    // Extract accounts from Vec<AccountInfo> passed to CPI
                    if let Some(account_infos_arg) = args.get(1) {
                        for account in mir_analyzer
                            .collect_accounts_from_account_infos_arg(account_infos_arg, false)
                        {
                            cpi_accounts.insert(account.account_name, bb);
                        }
                    }
                } else if cx
                    .tcx
                    .is_diagnostic_item(rustc_span::sym::deref_method, *fn_def_id)
                {
                    // Skip macro expansions
                    if fn_span.from_expansion() {
                        continue;
                    }
                    for account in args {
                        if let Operand::Move(account) = account.node
                            && let Some(local) = account.as_local()
                        {
                            // Check if the local is an account name
                            let account_name_and_locals = mir_analyzer
                                .check_local_and_assignment_locals(
                                    &local,
                                    &mut HashSet::new(),
                                    false,
                                    &mut String::new(),
                                );
                            for account_name_and_local in account_name_and_locals {
                                account_accesses
                                    .entry(account_name_and_local.account_name)
                                    .or_default()
                                    .push(AccountAccess {
                                        access_block: bb,
                                        access_span: *fn_span,
                                        stale_data_access: false,
                                    });
                            }
                        }
                    }
                }
                // CPI context
                else if DiagnoticItem::AnchorCpiContext.defid_is_type(cx.tcx, return_ty) {
                    if let Some(cpi_accounts_struct) = args.get(1)
                        && let Operand::Copy(place) | Operand::Move(place) =
                            &cpi_accounts_struct.node
                        && let Some(accounts_local) = place.as_local()
                        && let Some(accounts) = mir_analyzer
                            .find_cpi_accounts_struct(&accounts_local, &mut HashSet::new())
                    {
                        for account_local in accounts {
                            // Check if the local is an account name
                            if let Some(account_name_and_local) = extract_account_name_from_local(
                                &mir_analyzer,
                                &account_local,
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
                        mir_analyzer.get_nested_fn_arguments(args, None)
                {
                    // Called fn has reloads for its arguments
                    let nested_function_operations = analyze_nested_function_operations(
                        cx,
                        fn_def_id,
                        &fn_crate_name,
                        anchor_context_info,
                    );
                    let nested_cpi_calls = nested_function_operations.cpi_calls;
                    for cpi_call in nested_cpi_calls {
                        cpi_calls.insert(bb, cpi_call.cpi_call_span);
                    }
                    let nested_cpi_context_creation =
                        nested_function_operations.cpi_context_creation;
                    // Process nested CPI context creation and add them to cpi_accounts
                    process_nested_cpi_context_creation(
                        nested_cpi_context_creation,
                        &nested_argument,
                        anchor_context_info,
                        bb,
                        &mut cpi_accounts,
                    );
                    let nested_function_blocks = nested_function_operations.nested_function_blocks;
                    // Process nested function blocks and add them to account_reloads or account_accesses
                    process_nested_function_blocks(
                        nested_function_blocks,
                        &nested_argument,
                        anchor_context_info,
                        bb,
                        &mut account_reloads,
                        &mut account_accesses,
                    );
                }
            }
        }

        let cpi_call_blocks: HashSet<_> = cpi_calls.keys().copied().collect();

        // Filter accounts to only those involved in CPI calls
        cpi_accounts
            .retain(|_ty, &mut block| reachable_blocks(&mir.basic_blocks, block, &cpi_call_blocks));

        // Filter accounts to only those involved in CPI calls
        account_accesses.retain(|name, _| cpi_accounts.contains_key(name));

        // Filter out accounts that don't contain deserialized data
        let account_accesses =
            filter_account_accesses(cx, account_accesses, anchor_context_info, &cpi_accounts);

        for (_, accesses) in account_accesses.clone().iter() {
            for access in accesses.iter() {
                if access.stale_data_access {
                    // Check if this stale access is also reachable from CPI
                    let access_blocks = HashSet::from([access.access_block]);
                    let violations = reachable_without_passing(
                        &mir.basic_blocks,
                        cpi_call_blocks.clone(),
                        access_blocks,
                        HashSet::new(), // No reloads to check for stale accesses
                    );
                    if let Some(violation) = violations.first() {
                        trigger_missing_account_reload_lint_note(
                            cx,
                            access.access_span,
                            Some(cpi_calls[&violation.1]),
                        );
                    } else {
                        trigger_missing_account_reload_lint(cx, access.access_span);
                    }
                }
            }
        }

        for (ty, accesses) in account_accesses.into_iter() {
            // Check all accesses (both stale and non-stale) for CPI reachability
            let access_blocks: HashSet<BasicBlock> =
                accesses.iter().map(|access| access.access_block).collect();

            let reloads = account_reloads.remove(&ty).unwrap_or_default();

            for (access_block, cpi) in reachable_without_passing(
                &mir.basic_blocks,
                cpi_call_blocks.clone(),
                access_blocks,
                reloads,
            ) {
                if access_block == cpi {
                    continue;
                }
                for access in accesses.iter().filter(|a| a.access_block == access_block) {
                    trigger_missing_account_reload_lint_note(
                        cx,
                        access.access_span,
                        Some(cpi_calls[&cpi]),
                    );
                }
            }
        }
    }
}
pub fn trigger_missing_account_reload_lint(cx: &LateContext, access_span: Span) {
    span_lint(
        cx,
        MISSING_ACCOUNT_RELOAD,
        access_span,
        "accessing an account after a CPI without calling `reload()`",
    );
}
pub fn trigger_missing_account_reload_lint_note(
    cx: &LateContext,
    access_span: Span,
    cpi_span: Option<Span>,
) {
    span_lint_and_note(
        cx,
        MISSING_ACCOUNT_RELOAD,
        access_span,
        "accessing an account after a CPI without calling `reload()`",
        cpi_span,
        "CPI is here",
    );
}

// Recursively checks nested functions for account reload operations and returns account names with their types.
pub fn analyze_nested_function_operations<'tcx>(
    cx: &LateContext<'tcx>,
    fn_def_id: &DefId,
    fn_crate_name: &String,
    cpi_context_info: &AnchorContextInfo<'tcx>,
) -> NestedFunctionOperations<'tcx> {
    let mut nested_function_blocks: Vec<NestedFunctionBlocks<'tcx>> = Vec::new();
    let mut cpi_calls: Vec<CpiCallBlock> = Vec::new();
    let mut cpi_context_creation: Vec<CpiContextCreationBlock> = Vec::new();

    let local_def_id = fn_def_id.expect_local();
    let body_id = match get_hir_body_from_local_def_id(cx, local_def_id) {
        Some(body_id) => body_id,
        None => {
            return NestedFunctionOperations {
                nested_function_blocks: Vec::new(),
                cpi_calls: Vec::new(),
                cpi_context_creation: Vec::new(),
            };
        }
    };
    let body = cx.tcx.hir_body(body_id);
    let mir_analyzer = MirAnalyzer::new(cx, body, local_def_id);
    let mir_body = cx.tcx.optimized_mir(fn_def_id);
    let arg_names = get_nested_fn_arg_names(cx, *fn_def_id);

    for (bb, bbdata) in mir_body.basic_blocks.iter_enumerated() {
        if let TerminatorKind::Call {
            func: Operand::Constant(func),
            args,
            fn_span,
            ..
        } = &bbdata.terminator().kind
            && let rustc_ty::FnDef(def_id, _) = func.ty().kind()
        {
            let crate_name = cx.tcx.crate_name(def_id.krate).to_string();
            let fn_sig = cx.tcx.fn_sig(*def_id).skip_binder();
            let return_ty = fn_sig.skip_binder().output();

            // Handle Account::reload
            if DiagnoticItem::AnchorAccountReload.defid_is_item(cx.tcx, *def_id) {
                if let Some(block) = handle_account_reload_in_nested_function(
                    &mir_analyzer,
                    mir_body,
                    args,
                    *fn_span,
                    bb,
                ) {
                    nested_function_blocks.push(block);
                }
            }
            // Handle account access (deref)
            else if cx
                .tcx
                .is_diagnostic_item(rustc_span::sym::deref_method, *def_id)
            {
                if !fn_span.from_expansion() {
                    nested_function_blocks.extend(handle_account_access_in_nested_function(
                        cx,
                        &mir_analyzer,
                        mir_body,
                        args,
                        *fn_span,
                        bb,
                    ));
                }
            }
            // Handle CPI invoke or takes_cpi_context
            else if is_cpi_invoke_fn(cx.tcx, *def_id) || takes_cpi_context(cx, mir_body, args) {
                let (cpi_call, mut cpi_ctx_creation) = handle_cpi_invoke_in_nested_function(
                    &mir_analyzer,
                    args,
                    *fn_span,
                    bb,
                    &arg_names,
                );
                cpi_calls.push(cpi_call);
                cpi_context_creation.append(&mut cpi_ctx_creation);
            }
            // Handle CPI context creation
            else if DiagnoticItem::AnchorCpiContext.defid_is_type(cx.tcx, return_ty) {
                cpi_context_creation.extend(handle_cpi_context_creation_in_nested_function(
                    &mir_analyzer,
                    args,
                    bb,
                ));
            }
            // Handle nested function calls
            else if crate_name == *fn_crate_name
                && let Some(nested_argument) =
                    mir_analyzer.get_nested_fn_arguments(args, Some(cpi_context_info))
            {
                let (blocks, mut calls, mut ctx_creation) = handle_nested_function_call(
                    cx,
                    &mir_analyzer,
                    *def_id,
                    fn_crate_name,
                    cpi_context_info,
                    bb,
                    &nested_argument,
                );
                nested_function_blocks.extend(blocks);
                cpi_calls.append(&mut calls);
                cpi_context_creation.append(&mut ctx_creation);
            }
        }
    }

    // Post-processing
    if !nested_function_blocks.is_empty() {
        check_stale_data_accesses(mir_body, &mut nested_function_blocks);
    }
    if !cpi_calls.is_empty() && !nested_function_blocks.is_empty() {
        mark_unused_nested_reloads(mir_body, &mut nested_function_blocks, &cpi_calls);
    }

    NestedFunctionOperations {
        nested_function_blocks,
        cpi_calls,
        cpi_context_creation,
    }
}

fn is_cpi_invoke_fn(tcx: TyCtxt, def_id: DefId) -> bool {
    use DiagnoticItem::*;
    [
        AnchorCpiInvoke,
        AnchorCpiInvokeUnchecked,
        AnchorCpiInvokeSigned,
        AnchorCpiInvokeSignedUnchecked,
    ]
    .iter()
    .any(|item| item.defid_is_item(tcx, def_id))
}

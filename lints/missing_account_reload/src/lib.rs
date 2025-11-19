#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use std::collections::{HashMap, HashSet};

use clippy_utils::{
    diagnostics::{span_lint, span_lint_and_note},
    fn_has_unsatisfiable_preds,
    ty::is_type_diagnostic_item,
};

use rustc_hir::{
    Body as HirBody, FnDecl,
    def_id::{DefId, LocalDefId},
    intravisit::FnKind,
};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::{
    mir::{BasicBlock, HasLocalDecls, Operand, TerminatorKind},
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
        let mut account_accesses: HashMap<String, Vec<AccountAccess>> = HashMap::new();
        // Map of account fields to BBs reloading them
        let mut account_reloads: HashMap<String, HashSet<BasicBlock>> = HashMap::new();
        // Map of CPI context account types
        let mut cpi_accounts: HashMap<String, BasicBlock> = HashMap::new();

        // Map of account names invoked in a CPI & local map of assignments
        let (cpi_accounts_map, reverse_assignment_map) = build_local_relationship_maps(mir);
        let transitive_assignment_reverse_map =
            build_transitive_reverse_map(&reverse_assignment_map);
        let method_call_receiver_map = build_method_call_receiver_map(mir);
        let account_lookup_context = AccountLookupContext {
            cx,
            mir,
            transitive_assignment_reverse_map: &transitive_assignment_reverse_map,
            method_call_receiver_map: &method_call_receiver_map,
        };
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
                                &account_lookup_context,
                                &local,
                                &mut HashSet::new(),
                                false,
                                &mut String::new(),
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
                            &account_lookup_context,
                            &local,
                            &mut HashSet::new(),
                            false,
                            &mut String::new(),
                        )
                    {
                        account_accesses
                            .entry(account_name_and_local.account_name)
                            .or_default()
                            .push(AccountAccess {
                                access_block: bb,
                                access_span: *fn_span,
                                stale_data_access: false,
                            });
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
                                &account_lookup_context,
                                &account_local,
                                &mut HashSet::new(),
                                false,
                                &mut String::new(),
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
                    let nested_function_operations = analyze_nested_function_operations(
                        cx,
                        fn_def_id,
                        &fn_crate_name,
                        &anchor_context_info,
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
                        &anchor_context_info,
                        bb,
                        &mut cpi_accounts,
                    );
                    let nested_function_blocks = nested_function_operations.nested_function_blocks;
                    // Process nested function blocks and add them to account_reloads or account_accesses
                    process_nested_function_blocks(
                        nested_function_blocks,
                        &nested_argument,
                        &anchor_context_info,
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

        account_accesses.retain(|name, _| cpi_accounts.contains_key(name));
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
    let account_reload_sym = Symbol::intern("AnchorAccountReload");
    let deref_method_sym = Symbol::intern("deref_method");

    let cpi_invoke_syms = [
        Symbol::intern("AnchorCpiInvoke"),
        Symbol::intern("AnchorCpiInvokeUnchecked"),
        Symbol::intern("AnchorCpiInvokeSigned"),
        Symbol::intern("AnchorCpiInvokeSignedUnchecked"),
    ];
    let anchor_cpi_sym = Symbol::intern("AnchorCpiContext");

    let mut nested_function_blocks: Vec<NestedFunctionBlocks<'tcx>> = Vec::new();
    let mut cpi_calls: Vec<CpiCallBlock> = Vec::new();
    let mut cpi_context_creation: Vec<CpiContextCreationBlock> = Vec::new();

    let mir_body = cx.tcx.optimized_mir(fn_def_id);

    let (cpi_accounts_map, reverse_assignment_map) = build_local_relationship_maps(mir_body);
    let transitive_assignment_reverse_map = build_transitive_reverse_map(&reverse_assignment_map);
    let method_call_receiver_map = build_method_call_receiver_map(mir_body);
    let account_lookup_context = AccountLookupContext {
        cx,
        mir: mir_body,
        transitive_assignment_reverse_map: &transitive_assignment_reverse_map,
        method_call_receiver_map: &method_call_receiver_map,
    };
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
            let fn_sig_unbounded = fn_sig.skip_binder();
            let return_ty = fn_sig_unbounded.output();

            if let Some(diag_item) = cx.tcx.diagnostic_items(def_id.krate).id_to_name.get(def_id) {
                if *diag_item == account_reload_sym
                    && let Some(account) = args.first()
                    && let Operand::Move(account) = account.node
                    && let Some(local) = account.as_local()
                    && let Some(account_ty) =
                        mir_body.local_decls().get(local).map(|d| d.ty.peel_refs())
                {
                    if let Some(account_name_and_local) = check_local_and_assignment_locals(
                        &account_lookup_context,
                        &local,
                        &mut HashSet::new(),
                        true,
                        &mut String::new(),
                    ) {
                        let arg_local = resolve_to_original_local(
                            &account_name_and_local.account_local,
                            &mut HashSet::new(),
                            &transitive_assignment_reverse_map,
                        );
                        nested_function_blocks.push(NestedFunctionBlocks {
                            account_name: account_name_and_local.account_name.clone(),
                            account_ty,
                            account_local: arg_local,
                            account_span: *fn_span,
                            account_block: bb,
                            stale_data_access: false,
                            block_type: NestedBlockType::Reload,
                            not_used_reload: false,
                        });
                    }
                } else if *diag_item == deref_method_sym
                        && let Some(account) = args.first()
                        && let Operand::Move(account) = account.node
                        && let Some(local) = account.as_local()
                        && let Some(account_ty) =
                        mir_body.local_decls().get(local).map(|d| d.ty.peel_refs())
                        // Check if the local is an account name
                        && let Some(account_name_and_local) = check_local_and_assignment_locals(
                            &account_lookup_context,
                            &local,
                            &mut HashSet::new(),
                            true,
                            &mut String::new(),
                        )
                {
                    let arg_local = resolve_to_original_local(
                        &account_name_and_local.account_local,
                        &mut HashSet::new(),
                        &transitive_assignment_reverse_map,
                    );
                    nested_function_blocks.push(NestedFunctionBlocks {
                        account_name: account_name_and_local.account_name,
                        account_ty,
                        account_local: arg_local,
                        account_span: *fn_span,
                        account_block: bb,
                        stale_data_access: false,
                        block_type: NestedBlockType::Access,
                        not_used_reload: false,
                    });
                } else if cpi_invoke_syms.contains(diag_item) {
                    cpi_calls.push(CpiCallBlock {
                        cpi_call_block: bb,
                        cpi_call_span: *fn_span,
                    });
                }
            } else if takes_cpi_context(cx, mir_body, args) {
                cpi_calls.push(CpiCallBlock {
                    cpi_call_block: bb,
                    cpi_call_span: *fn_span,
                });
            } else if is_type_diagnostic_item(cx, return_ty, anchor_cpi_sym) {
                if let Some(cpi_accounts_struct) = args.get(1)
                    && let Operand::Copy(place) | Operand::Move(place) = &cpi_accounts_struct.node
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
                            &account_lookup_context,
                            &account_local,
                            &mut HashSet::new(),
                            true,
                            &mut String::new(),
                        ) && let Some(cpi_context_block) = create_cpi_context_creation_block(
                            account_name_and_local.clone(),
                            bb,
                            mir_body,
                            &transitive_assignment_reverse_map,
                        ) {
                            cpi_context_creation.push(cpi_context_block);
                        }
                    }
                }
            // Check if the function is a nested function
            } else if crate_name == *fn_crate_name
                && let Some(nested_argument) =
                    get_nested_fn_arguments(cx, mir_body, args, cpi_context_info)
            {
                // Analyze nested function operations
                let nested_function_operations =
                    analyze_nested_function_operations(cx, def_id, fn_crate_name, cpi_context_info);

                // Analyze reloads and accesses in the nested function
                let nested_blocks = nested_function_operations.nested_function_blocks;
                let nested_function_blocks_clone =
                    remap_nested_function_blocks(nested_blocks, &nested_argument, bb);
                nested_function_blocks.extend(nested_function_blocks_clone);

                // Analyze CPI context creation in the nested function
                let nested_cpi_context_creation = nested_function_operations.cpi_context_creation;
                merge_nested_cpi_context_creation(
                    nested_cpi_context_creation,
                    &nested_argument,
                    &mut cpi_context_creation,
                );

                // Analyze CPI calls in the nested function
                let nested_cpi_calls = nested_function_operations.cpi_calls;
                for cpi_call in nested_cpi_calls {
                    cpi_calls.push(CpiCallBlock {
                        cpi_call_block: bb,
                        cpi_call_span: cpi_call.cpi_call_span,
                    });
                }
            }
        }
    }

    // If there are account reloads and accesses, check if the access is dominated by the reload
    if !nested_function_blocks.is_empty() {
        check_stale_data_accesses(mir_body, &mut nested_function_blocks);
    }

    // If there are CPI calls & reloads, check if the reload is not used
    if !cpi_calls.is_empty() && !nested_function_blocks.is_empty() {
        mark_unused_nested_reloads(mir_body, &mut nested_function_blocks, &cpi_calls);
    }
    NestedFunctionOperations {
        nested_function_blocks,
        cpi_calls,
        cpi_context_creation,
    }
}

#[test]
fn test_missing_account_reload() {
    dylint_testing::ui_test_example(env!("CARGO_PKG_NAME"), "missing_account_reload");
}

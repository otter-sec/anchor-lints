#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use anchor_lints_utils::{
    diag_items::DiagnoticItem, models::NestedArgument, utils::get_hir_body_from_local_def_id,
};

use clippy_utils::{diagnostics::span_lint, fn_has_unsatisfiable_preds};
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
use rustc_span::{Span, sym};

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

mod models;
mod utils;

use models::{Cmp, CpiCallsInfo, CpiContextsInfo, IfThen};
use utils::*;

use anchor_lints_utils::mir_analyzer::MirAnalyzer;
use anchor_lints_utils::models::Origin;

dylint_linting::declare_late_lint! {
    /// ### What it does
    /// Detects potential **arbitrary Cross-Program Invocations (CPIs)** where the target
    /// program ID appears to be user-controlled without validation.
    ///
    /// ### Why is this bad?
    /// Allowing user-controlled program ID in CPI calls can lead to
    /// **security vulnerabilities**, such as unauthorized fund transfers, privilege
    /// escalation, or unintended external calls. All CPI targets should be strictly
    /// validated or hardcoded to ensure safe execution.
    ///
    pub ARBITRARY_CPI_CALL,
    Warn,
    "arbitrary CPI detected — target program ID may be user-controlled"
}

thread_local! {
    // Track functions that are being analyzed during recursive calls
    static RECURSIVE_ANALYSIS: RefCell<HashSet<LocalDefId>> = RefCell::new(HashSet::new());

    // Track functions that have been analyzed (to skip duplicate standalone analysis)
    static ANALYZED_FUNCTIONS: RefCell<HashSet<LocalDefId>> = RefCell::new(HashSet::new());
}

impl<'tcx> LateLintPass<'tcx> for ArbitraryCpiCall {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        _kind: FnKind<'tcx>,
        _: &'tcx FnDecl<'tcx>,
        body: &'tcx HirBody<'tcx>,
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

        // Skip if already being analyzed recursively
        let is_recursive = RECURSIVE_ANALYSIS.with(|rec| rec.borrow().contains(&def_id));
        if is_recursive {
            return;
        }

        // Skip if already analyzed (during recursive analysis)
        let already_analyzed =
            ANALYZED_FUNCTIONS.with(|analyzed| analyzed.borrow().contains(&def_id));
        if already_analyzed {
            return;
        }

        let fn_crate_name = cx.tcx.crate_name(def_id.to_def_id().krate).to_string();
        let mut account_cmps: Vec<String> = vec![];
        analyze_arbitrary_cpi_call(
            cx,
            body,
            def_id,
            &fn_crate_name,
            false,
            None,
            &mut account_cmps,
        );
    }
}

fn analyze_arbitrary_cpi_call<'tcx>(
    cx: &LateContext<'tcx>,
    body: &HirBody<'tcx>,
    def_id: LocalDefId,
    fn_crate_name: &String,
    call_from_helper: bool,
    existing_nested_arg_accounts: Option<NestedArgument<'tcx>>,
    existing_account_cmps: &mut Vec<String>,
) -> Vec<String> {
    // Mark as being analyzed recursively
    RECURSIVE_ANALYSIS.with(|rec| {
        rec.borrow_mut().insert(def_id);
    });

    // Mark as analyzed
    ANALYZED_FUNCTIONS.with(|analyzed| {
        analyzed.borrow_mut().insert(def_id);
    });
    let mut mir_analyzer = MirAnalyzer::new(cx, body, def_id);

    // If fn does not take a anchor context, skip to avoid false positives
    if mir_analyzer.anchor_context_info.is_none() {
        if call_from_helper {
            mir_analyzer.update_anchor_context_info_with_context_accounts(body);
        }
        if mir_analyzer.anchor_context_info.is_none() && !call_from_helper {
            return vec![];
        }
    }
    if let Some(existing_nested_arg_accounts) = &existing_nested_arg_accounts
        && !existing_account_cmps.is_empty()
    {
        map_nested_arg_accounts_to_account_cmps(
            existing_nested_arg_accounts,
            &mir_analyzer.param_info,
            existing_account_cmps,
        );
    }

    let mir = mir_analyzer.mir;
    // Need to identify:
    // A) CPI calls
    // B) CPI contexts with user controllable program id
    // C) Conditional blocks for program id
    // Then we check all CPI contexts where a CPI call is reachable from the context
    // and the program ID is not validated in any conditional blocks

    let mut cpi_calls: HashMap<BasicBlock, CpiCallsInfo> = HashMap::new();
    let mut cpi_contexts: HashMap<BasicBlock, CpiContextsInfo> = HashMap::new();
    let mut switches: Vec<IfThen> = Vec::new();
    let mut program_id_cmps: Vec<Cmp> = Vec::new();

    let mut instruction_to_program_id: HashMap<Local, BasicBlock> = HashMap::new();

    for (bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        for statement in &bbdata.statements {
            record_instruction_creation(
                &mir_analyzer,
                bb,
                statement,
                &mut instruction_to_program_id,
            );
        }
        let terminator_kind = &bbdata.terminator().kind;
        if let TerminatorKind::Call {
            func: Operand::Constant(func_const),
            args,
            fn_span,
            destination,
            ..
        } = terminator_kind
            && let rustc_ty::FnDef(fn_def_id, _) = func_const.ty().kind()
        {
            let fn_sig = cx.tcx.fn_sig(*fn_def_id).skip_binder();
            let return_ty = fn_sig.skip_binder().output();

            let crate_name = cx.tcx.crate_name(fn_def_id.krate).to_string();

            if is_cpi_invoke_fn(cx.tcx, *fn_def_id) {
                if let Some(instruction) = args.first()
                    && let Operand::Copy(place) | Operand::Move(place) = &instruction.node
                    && let Some(instruction_local) = place.as_local()
                {
                    // Check if this is an Instruction type
                    track_instruction_call(
                        &mir_analyzer,
                        instruction_local,
                        *fn_span,
                        bb,
                        &mut cpi_calls,
                        &mut cpi_contexts,
                        &instruction_to_program_id,
                    );
                }
            // if not a CPI invoke function, check if the function takes a CPI context, and if it does, extract the CPI context local
            } else if mir_analyzer.takes_cpi_context(args)
                && let Some(instruction) = args.first()
                && let Operand::Copy(place) | Operand::Move(place) = &instruction.node
                && let Some(local) = place.as_local()
                && let Some(ty) = mir.local_decls().get(local).map(|d| d.ty.peel_refs())
                && is_anchor_cpi_context(cx, ty)
                && !is_anchor_spl_token_transfer(cx, *fn_def_id)
            {
                if let Some(cpi_ctx_local) = get_local_from_operand(args.first()) {
                    cpi_calls.insert(
                        bb,
                        CpiCallsInfo {
                            span: *fn_span,
                            local: cpi_ctx_local,
                        },
                    );
                }
            // check if the function returns a CPI context
            } else if is_anchor_cpi_context(cx, return_ty) {
                // check if CPI context with user controllable program id
                if let Some(program_id) = args.first()
                    && let Operand::Copy(place) | Operand::Move(place) = &program_id.node
                    && let Some(local) = place.as_local()
                    && mir_analyzer.is_pubkey_type(local)
                    && let Some(cpi_ctx_return_local) = destination.as_local()
                    && matches!(
                        mir_analyzer.origin_of_operand(&program_id.node),
                        Origin::Parameter | Origin::Unknown
                    )
                {
                    cpi_contexts.insert(
                        bb,
                        CpiContextsInfo {
                            cpi_ctx_local: cpi_ctx_return_local,
                            program_id_local: local,
                        },
                    );
                }
            } else if cx.tcx.is_diagnostic_item(sym::cmp_partialeq_eq, *fn_def_id)
                && let Some((lhs, rhs)) = mir_analyzer.args_as_pubkey_locals(args)
                && let Some(ret) = destination.as_local()
            {
                program_id_cmps.push(Cmp {
                    lhs,
                    rhs,
                    ret,
                    is_eq: true,
                });
            } else if let [_receiver, arg] = args.as_ref()
                && let Some(maybe_pubkey) = mir_analyzer.pubkey_operand_to_local(&arg.node)
                && let Some(name) = cx.tcx.opt_item_name(*fn_def_id)
                && name.as_str() == "contains"
                && return_ty.is_bool()
                && let Some(ret) = destination.as_local()
            {
                // FIXME: Represent this more accurately than a fake comparison to self
                program_id_cmps.push(Cmp {
                    lhs: maybe_pubkey,
                    rhs: maybe_pubkey,
                    ret,
                    is_eq: true,
                });
            } else if cx.tcx.is_diagnostic_item(sym::cmp_partialeq_ne, *fn_def_id)
                && let Some((lhs, rhs)) = mir_analyzer.args_as_pubkey_locals(args)
                && let Some(ret) = destination.as_local()
            {
                program_id_cmps.push(Cmp {
                    lhs,
                    rhs,
                    ret,
                    is_eq: false,
                });
            } else if crate_name == *fn_crate_name && !fn_span.from_expansion() {
                // filter out program id comparisons that are not reachable & unsafe comparisons
                let filtered_program_id_cmps =
                    filter_program_id_cmps(bb, &program_id_cmps, &switches, &mir_analyzer);
                add_program_id_to_existing_account_cmps(
                    &filtered_program_id_cmps,
                    &mir_analyzer,
                    existing_account_cmps,
                );
                let local_def_id = fn_def_id.expect_local();
                let body_id = match get_hir_body_from_local_def_id(cx, local_def_id) {
                    Some(body_id) => body_id,
                    None => {
                        continue;
                    }
                };
                let body = cx.tcx.hir_body(body_id);
                let nested_arg_accounts = mir_analyzer
                    .get_nested_fn_arguments(args, None)
                    .or(mir_analyzer.get_nested_fn_arguments_as_params(args));
                let nested_program_id_cmps = analyze_arbitrary_cpi_call(
                    cx,
                    body,
                    local_def_id,
                    fn_crate_name,
                    true,
                    nested_arg_accounts.clone(),
                    existing_account_cmps,
                );
                if !nested_program_id_cmps.is_empty() {
                    existing_account_cmps.extend(nested_program_id_cmps);
                }
            }
        }
        // Find if/else switches which may be the result of a comparison
        else if let TerminatorKind::SwitchInt {
            discr: Operand::Move(discr),
            targets,
        } = terminator_kind
            && let Some(discr) = discr.as_local()
            && let Some(discr_decl) = mir.local_decls().get(discr)
            && discr_decl.ty.is_bool()
            && let Some((val, then, els)) = targets.as_static_if()
        {
            let then_block = if val == 1 { then } else { els };
            let else_block = if then_block == then { els } else { then };
            switches.push(IfThen {
                discr,
                then: then_block,
                els: else_block,
            });
        }
    }
    // check if the CPI call is reachable from a CPI context
    // and the program ID is not validated in conditional blocks
    for (bb, cpi_ctx_info) in cpi_contexts.into_iter() {
        // check if the CPI call is reachable from the CPI context
        let Some(cpi_call_bb) =
            cpi_invocation_is_reachable_from_cpi_context(&mir.basic_blocks, bb, &cpi_calls)
        else {
            continue;
        };

        // check if the CPI context variables are the same
        if !mir_analyzer.check_cpi_context_variables_are_same(
            &cpi_ctx_info.cpi_ctx_local,
            &cpi_calls[&cpi_call_bb].local,
            &mut HashSet::new(),
        ) {
            continue;
        }

        // Check if program ID is validated
        let check_program_id_validated = pubkey_checked_in_this_block(
            cpi_call_bb,
            cpi_ctx_info.program_id_local,
            &program_id_cmps,
            &switches,
            &mir_analyzer,
        ) || !check_program_id_included_in_conditional_blocks(
            &cpi_ctx_info.program_id_local,
            &program_id_cmps,
            &mir_analyzer,
        );

        if !check_program_id_validated {
            continue;
        }

        // Check if account was validated in previous blocks (inter-procedural)
        if is_account_checked_in_previous_blocks(
            &cpi_ctx_info.program_id_local,
            existing_account_cmps,
            &mir_analyzer,
        ) {
            continue;
        }

        span_lint(
            cx,
            ARBITRARY_CPI_CALL,
            cpi_calls[&cpi_call_bb].span,
            "arbitrary CPI detected — program id appears user-controlled",
        );
    }
    // Unmark as being analyzed recursively
    RECURSIVE_ANALYSIS.with(|rec| {
        rec.borrow_mut().remove(&def_id);
    });

    add_program_id_to_existing_account_cmps(&program_id_cmps, &mir_analyzer, existing_account_cmps);
    if let Some(existing_nested_arg_accounts) = &existing_nested_arg_accounts
        && !existing_account_cmps.is_empty()
    {
        map_param_info_to_nested_accounts(
            existing_nested_arg_accounts,
            &mir_analyzer.param_info,
            existing_account_cmps,
        );
    }
    existing_account_cmps.clone()
}

/// For a given pubkey [`Local`], identify the [`BasicBlock`]s where its value is known/checked
fn known_pubkey_basic_blocks<'tcx>(
    pk: Local,
    cmps: &[Cmp],
    switches: &[IfThen],
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
) -> Vec<BasicBlock> {
    cmps.iter()
        .filter_map(|cmp| {
            let is_same = |lhs: Local, rhs: Local| -> bool {
                mir_analyzer
                    .transitive_assignment_reverse_map
                    .values()
                    .any(|v| v.contains(&lhs) && v.contains(&rhs))
                    || mir_analyzer.are_same_account(lhs, rhs)
            };

            (is_same(cmp.lhs, pk) || is_same(cmp.rhs, pk)).then_some((cmp.ret, cmp.is_eq))
        })
        // Find switches on the comparison result, then get the truthy blocks
        .flat_map(|cmp_res| {
            switches.iter().filter_map(move |switch| {
                (switch.discr == cmp_res.0).then_some(if cmp_res.1 {
                    switch.then
                } else {
                    switch.els
                })
            })
        })
        .collect()
}
/// Check if `pk` has been checked to be a known value at the point this basic block is reached
fn pubkey_checked_in_this_block<'tcx>(
    block: BasicBlock,
    pk: Local,
    cmps: &[Cmp],
    switches: &[IfThen],
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
) -> bool {
    let known_bbs = known_pubkey_basic_blocks(pk, cmps, switches, mir_analyzer);
    known_bbs
        .iter()
        .any(|bb| !mir_analyzer.dominators.dominates(*bb, block))
}

fn is_anchor_cpi_context<'tcx>(cx: &LateContext<'tcx>, ty: rustc_ty::Ty<'tcx>) -> bool {
    DiagnoticItem::AnchorCpiContext.defid_is_type(cx.tcx, ty)
}

fn is_anchor_spl_token_transfer<'tcx>(cx: &LateContext<'tcx>, def_id: DefId) -> bool {
    DiagnoticItem::AnchorSplTokenTransfer.defid_is_item(cx.tcx, def_id)
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

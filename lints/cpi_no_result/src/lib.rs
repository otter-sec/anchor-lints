#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use anchor_lints_utils::{
    diag_items::{is_anchor_cpi_context, is_cpi_invoke_fn},
    mir_analyzer::MirAnalyzer,
};

use clippy_utils::{
    diagnostics::span_lint,
    fn_has_unsatisfiable_preds,
    sym::{Result, unwrap_or, unwrap_or_default, unwrap_or_else},
};
use rustc_hir::{Body as HirBody, FnDecl, def_id::LocalDefId, intravisit::FnKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::{
    mir::{BasicBlock, HasLocalDecls, Local, Operand, TerminatorKind},
    ty::{self as rustc_ty, TyKind},
};

use rustc_span::{Span, Symbol, source_map::Spanned};

dylint_linting::declare_late_lint! {
    /// ### What it does
    /// Detects **Cross-Program Invocation (CPI)** where the result is silently suppressed
    /// using methods like `unwrap_or_default()` or `unwrap_or(())`.
    ///
    /// ### Why is this bad?
    /// CPI calls can fail for various reasons (insufficient funds, invalid accounts, program errors, etc.).
    /// Silent suppression methods hide these failures, allowing the program to continue execution even when
    /// critical operations failed, leading to:
    /// - Silent failures that go unnoticed
    /// - Security vulnerabilities from unexpected program state
    /// - Potential fund loss from failed transfers
    /// - State corruption from invalid assumptions
    ///
    /// ### Example
    /// ```rust
    /// Bad: Error silently suppressed
    /// system_program::transfer(cpi_context, amount).unwrap_or_default();
    ///
    /// Good: Error properly handled
    /// system_program::transfer(cpi_context, amount)?;
    /// ```
    pub CPI_NO_RESULT,
    Warn,
    "CPI call result is silently suppressed"
}

impl<'tcx> LateLintPass<'tcx> for CpiNoResult {
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

        // Analyze the function for CPI calls without result handling
        analyze_cpi_no_result(cx, body, def_id);
    }
}

fn analyze_cpi_no_result<'tcx>(cx: &LateContext<'tcx>, body: &HirBody<'tcx>, def_id: LocalDefId) {
    let mir = cx.tcx.optimized_mir(def_id.to_def_id());
    let mir_analyzer = MirAnalyzer::new(cx, body, def_id);
    let mut cpi_calls_with_silent_suppression: Vec<(BasicBlock, Span)> = Vec::new();

    for (bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        let terminator = bbdata.terminator();

        if let TerminatorKind::Call {
            func: Operand::Constant(func_const),
            args,
            destination,
            fn_span,
            target,
            ..
        } = &terminator.kind
            && let rustc_ty::FnDef(fn_def_id, _) = func_const.ty().kind()
            && check_is_cpi_call(&mir_analyzer, args, *fn_def_id)
        {
            // Check if the result is used in a silent error suppression method call
            if let Some(dest_local) = destination.as_local() {
                // Check the current block's statements (for chained calls in same block)
                if is_silent_error_suppression_in_block(&mir_analyzer, bb, dest_local) {
                    cpi_calls_with_silent_suppression.push((bb, *fn_span));
                    continue;
                }

                // Check the target block (where execution continues after CPI call)
                if let Some(target_bb) = *target
                    && is_silent_error_suppression_in_block(&mir_analyzer, target_bb, dest_local)
                {
                    cpi_calls_with_silent_suppression.push((bb, *fn_span));
                    continue;
                }

                // Also check all blocks for method calls on this result
                if is_silent_error_suppression(&mir_analyzer, dest_local) {
                    cpi_calls_with_silent_suppression.push((bb, *fn_span));
                }
            }
        }
    }

    // Emit warnings for silent error suppression
    for (_, cpi_span) in cpi_calls_with_silent_suppression {
        span_lint(
            cx,
            CPI_NO_RESULT,
            cpi_span,
            "CPI call result seems to be silently suppressed. Use `?` operator or explicit error handling instead.",
        );
    }
}

fn check_is_cpi_call(
    mir_analyzer: &MirAnalyzer,
    args: &[Spanned<Operand>],
    func_def_id: rustc_hir::def_id::DefId,
) -> bool {
    // First check if it's a known CPI invoke function (invoke, invoke_signed, etc.)
    if is_cpi_invoke_fn(mir_analyzer.cx.tcx, func_def_id) {
        return true;
    }

    // Check if it takes CpiContext as first argument
    if !mir_analyzer.takes_cpi_context(args) {
        return false;
    }

    // Get the function's return type
    let fn_sig = mir_analyzer.cx.tcx.fn_sig(func_def_id).skip_binder();
    let return_ty = fn_sig.skip_binder().output();

    if let rustc_ty::TyKind::Adt(_, _) = return_ty.kind()
        && is_anchor_cpi_context(mir_analyzer.cx.tcx, return_ty)
    {
        return false; // This is a builder method, not a CPI call
    }

    // Check if first argument is CpiContext
    if let Some(instruction) = args.first()
        && let Operand::Copy(place) | Operand::Move(place) = &instruction.node
        && let Some(local) = place.as_local()
        && let Some(ty) = mir_analyzer
            .mir
            .local_decls()
            .get(local)
            .map(|d| d.ty.peel_refs())
        && is_anchor_cpi_context(mir_analyzer.cx.tcx, ty)
    {
        return true;
    }

    false
}

// Check if a method name is a silent suppression method
fn is_suppression_method_name(name: Symbol) -> bool {
    name == unwrap_or_default || name == unwrap_or || name == unwrap_or_else
}

// Check if a function def_id is a Result suppression method
fn is_result_suppression_method<'tcx>(
    tcx: rustc_middle::ty::TyCtxt<'tcx>,
    def_id: rustc_hir::def_id::DefId,
) -> bool {
    let Some(result_adt) = tcx.get_diagnostic_item(Result) else {
        return false;
    };

    for impl_def_id in tcx.inherent_impls(result_adt) {
        let assoc = tcx.associated_items(*impl_def_id);
        for item in assoc.in_definition_order() {
            if item.def_id == def_id && is_suppression_method_name(item.name()) {
                return true;
            }
        }
    }
    false
}

// Check if a call terminator is a suppression method on a specific local
fn is_call_suppression_method_on_local(
    mir_analyzer: &MirAnalyzer,
    term: &rustc_middle::mir::Terminator,
    target_local: Local,
) -> bool {
    use std::collections::HashSet;

    if let TerminatorKind::Call { func, args, .. } = &term.kind {
        // Check if receiver matches target local
        if let Some(receiver) = args.first()
            && let Operand::Copy(place) | Operand::Move(place) = &receiver.node
            && let Some(receiver_local) = place.as_local()
        {
            let resolved_target =
                mir_analyzer.resolve_to_original_local(target_local, &mut HashSet::new());
            let resolved_receiver =
                mir_analyzer.resolve_to_original_local(receiver_local, &mut HashSet::new());

            if resolved_receiver == resolved_target
                && let Operand::Constant(func_const) = func
                && let TyKind::FnDef(def_id, _) = func_const.ty().kind()
            {
                return is_result_suppression_method(mir_analyzer.cx.tcx, *def_id);
            }
        }
    }
    false
}

// Check for silent error suppression methods
fn is_silent_error_suppression(mir_analyzer: &MirAnalyzer, cpi_result_local: Local) -> bool {
    use std::collections::HashSet;
    let resolved_local =
        mir_analyzer.resolve_to_original_local(cpi_result_local, &mut HashSet::new());

    for (method_result_local, receiver_local) in &mir_analyzer.method_call_receiver_map {
        let receiver_resolved =
            mir_analyzer.resolve_to_original_local(*receiver_local, &mut HashSet::new());
        if receiver_resolved == resolved_local {
            // Find the terminator for this method call
            for (_bb, bbdata) in mir_analyzer.mir.basic_blocks.iter_enumerated() {
                if let Some(term) = &bbdata.terminator
                    && let TerminatorKind::Call { destination, .. } = &term.kind
                    && destination.as_local() == Some(*method_result_local)
                    && is_call_suppression_method_on_local(mir_analyzer, term, resolved_local)
                {
                    return true;
                }
            }
        }
    }

    // Check all blocks directly
    for (_bb, bbdata) in mir_analyzer.mir.basic_blocks.iter_enumerated() {
        if let Some(term) = &bbdata.terminator
            && is_call_suppression_method_on_local(mir_analyzer, term, resolved_local)
        {
            return true;
        }
    }
    false
}

// Check for silent error suppression in a specific block
fn is_silent_error_suppression_in_block(
    mir_analyzer: &MirAnalyzer,
    target_bb: BasicBlock,
    cpi_result_local: Local,
) -> bool {
    use std::collections::HashSet;
    let resolved_local =
        mir_analyzer.resolve_to_original_local(cpi_result_local, &mut HashSet::new());

    let bbdata = &mir_analyzer.mir.basic_blocks[target_bb];
    if let Some(term) = &bbdata.terminator {
        return is_call_suppression_method_on_local(mir_analyzer, term, resolved_local);
    }
    false
}

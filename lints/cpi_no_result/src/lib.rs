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
    sym::{Result, expect, is_err, ok, unwrap},
};
use rustc_hir::{Body as HirBody, FnDecl, def_id::LocalDefId, intravisit::FnKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::{
    mir::{
        BasicBlock, HasLocalDecls, Local, Operand, Place, Rvalue, StatementKind, TerminatorKind,
    },
    ty::{self as rustc_ty, TyKind},
};

use rustc_span::{Span, source_map::Spanned};

dylint_linting::declare_late_lint! {
    /// ### What it does
    /// Detects **Cross-Program Invocation (CPI)** calls where the result is not properly handled.
    ///
    /// ### Why is this bad?
    /// CPI calls can fail for various reasons (insufficient funds, invalid accounts, program errors, etc.).
    /// If the result is not checked, the program may continue execution even when the CPI failed, leading to:
    /// - Silent failures that go unnoticed
    /// - Security vulnerabilities from unexpected program state
    /// - Potential fund loss from failed transfers
    /// - State corruption from invalid assumptions
    ///
    /// ### Example
    /// ```rust
    /// Bad: Result not handled
    /// system_program::transfer(cpi_context, amount); // Missing `?` or error handling
    ///
    /// Good: Result properly handled
    /// system_program::transfer(cpi_context, amount)?; // Result handled with `?`
    /// ```
    pub CPI_NO_RESULT,
    Warn,
    "CPI call result is not properly handled"
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
    let mut cpi_calls_without_result: Vec<(BasicBlock, Span)> = Vec::new();

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
            // SAFE CASE: CPI result is directly returned by the function
            if destination.as_local() == Some(rustc_middle::mir::RETURN_PLACE) {
                continue;
            }

            // SAFE CASE: CPI result is explicitly discarded with `let _ = ...`
            if let Some(dest_local) = destination.as_local()
                && is_local_never_read(&mir_analyzer, dest_local)
            {
                continue; // User explicitly discarded the result
            }

            // SAFE CASE: CPI result is handled in another block
            if let Some(target_bb) = *target
                && check_is_cpi_call_result_handled(&mir_analyzer, target_bb, destination)
            {
                continue;
            }
            cpi_calls_without_result.push((bb, *fn_span));
        }
    }
    // Emit warnings for CPI calls without result handling
    for (_, cpi_span) in cpi_calls_without_result {
        span_lint(
            cx,
            CPI_NO_RESULT,
            cpi_span,
            "CPI call result is not handled. Consider using `?` operator or explicit error handling.",
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

    // If the function returns a CpiContext, it's a builder method (like .with_signer()),
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

fn check_is_cpi_call_result_handled(
    mir_analyzer: &MirAnalyzer,
    target_bb: BasicBlock,
    destination: &Place,
) -> bool {
    // Check if the CPI call result is handled
    if is_try_branch(mir_analyzer, target_bb)
        || is_unwrap_or_expect(mir_analyzer, target_bb)
        || is_switch_on_result(mir_analyzer, target_bb, destination.as_local())
    {
        return true;
    }
    false
}

// Handles try branch result
fn is_try_branch(mir_analyzer: &MirAnalyzer, bb: BasicBlock) -> bool {
    let bbdata = &mir_analyzer.mir.basic_blocks[bb];
    if let Some(term) = &bbdata.terminator
        && let TerminatorKind::Call { func, .. } = &term.kind
        && let Operand::Constant(func_const) = func
        && let TyKind::FnDef(def_id, _) = func_const.ty().kind()
        && let Some(try_branch_def_id) = mir_analyzer.cx.tcx.lang_items().branch_fn()
        && *def_id == try_branch_def_id
    {
        return true;
    }
    false
}

// Handles unwrap, expect or is_err expression result
fn is_unwrap_or_expect(mir_analyzer: &MirAnalyzer, bb: BasicBlock) -> bool {
    let bbdata = &mir_analyzer.mir.basic_blocks[bb];
    if let Some(term) = &bbdata.terminator
        && let TerminatorKind::Call {
            func,
            target,
            destination,
            ..
        } = &term.kind
        && let Operand::Constant(func_const) = func
        && let TyKind::FnDef(def_id, _) = func_const.ty().kind()
    {
        let tcx = mir_analyzer.cx.tcx;
        let Some(result_adt) = tcx.get_diagnostic_item(Result) else {
            return false;
        };

        for impl_def_id in tcx.inherent_impls(result_adt) {
            let assoc = tcx.associated_items(*impl_def_id);

            for item in assoc.in_definition_order() {
                if item.def_id == *def_id {
                    if item.name() == unwrap || item.name() == expect || item.name() == is_err {
                        return true;
                    } else if let Some(target_bb) = target
                        && is_try_branch(mir_analyzer, *target_bb)
                    {
                        return true;
                    } else if let Some(target_bb) = target
                        && item.name() == ok
                    {
                        return is_switch_on_result(
                            mir_analyzer,
                            *target_bb,
                            destination.as_local(),
                        );
                    }
                }
            }
        }
    }
    false
}

// Handles switch on result
fn is_switch_on_result(
    mir_analyzer: &MirAnalyzer,
    bb: BasicBlock,
    cpi_result_local: Option<Local>,
) -> bool {
    let bbdata = &mir_analyzer.mir.basic_blocks[bb];

    if let Some(term) = &bbdata.terminator
        && let TerminatorKind::SwitchInt { discr, targets, .. } = &term.kind
        && let Some(discr_local) = discr.place()
    {
        for stmt in &bbdata.statements {
            if let StatementKind::Assign(box (place, Rvalue::Discriminant(place_ref))) = &stmt.kind
                && Some(place_ref.as_local()) == Some(cpi_result_local)
                && place.as_local() == discr_local.as_local()
            {
                return targets.iter().any(|(value, _)| value == 0 || value == 1); // Ok & Err case
            }
        }
    }

    false
}

// Check if a local is never read
fn is_local_never_read(mir_analyzer: &MirAnalyzer, local: Local) -> bool {
    let mir = mir_analyzer.mir;

    // Check all basic blocks for any use of this local
    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        // Check statements
        for stmt in &bbdata.statements {
            if let StatementKind::Assign(box (_, rvalue)) = &stmt.kind {
                // Check if local is used in the RHS
                if rvalue_uses_local(rvalue, local) {
                    return false;
                }
            }
        }

        // Check terminator
        if let Some(term) = &bbdata.terminator
            && terminator_uses_local(term, local)
        {
            return false;
        }
    }

    true
}

fn rvalue_uses_local(rvalue: &Rvalue, local: Local) -> bool {
    match rvalue {
        Rvalue::Use(Operand::Copy(place) | Operand::Move(place)) => {
            place.as_local() == Some(local)
        }
        Rvalue::Use(Operand::Constant(_)) => false,
        Rvalue::Ref(_, _, place) => place.as_local() == Some(local),
        Rvalue::Aggregate(_, operands) => {
            operands.iter().any(|op| {
                matches!(op, Operand::Copy(place) | Operand::Move(place) if place.as_local() == Some(local))
            })
        }
        _ => false,
    }
}

fn terminator_uses_local(term: &rustc_middle::mir::Terminator, local: Local) -> bool {
    match &term.kind {
        TerminatorKind::Call { args, .. } => {
            args.iter().any(|arg| {
                matches!(&arg.node, Operand::Copy(place) | Operand::Move(place) if place.as_local() == Some(local))
            })
        }
        TerminatorKind::SwitchInt { discr, .. } => {
            if let Operand::Copy(place) | Operand::Move(place) = &discr
                && let Some(discr_local) = place.as_local()
            {
                discr_local == local
            } else {
                false
            }
        }
        _ => false,
    }
}

use anchor_lints_utils::diag_items::is_anchor_cpi_context;
use anchor_lints_utils::models::{PdaSigner, UnsafeAccount};
use rustc_hir::def_id::LocalDefId;
use rustc_middle::mir::{Operand, TerminatorKind};
use rustc_middle::ty as rustc_ty;
use rustc_span::{Span, source_map::Spanned};

use anchor_lints_utils::mir_analyzer::{AnchorContextInfo, MirAnalyzer};
use clippy_utils::diagnostics::span_lint_and_help;

use crate::utils::has_constraint_preventing_overlap;
use crate::{
    PDA_SIGNER_ACCOUNT_OVERLAP, analyze_nested_function_if_available,
    check_cpi_call_is_new_with_signer, check_cpi_uses_pda_signer, extract_accounts_passed_to_cpi,
    is_implementation_method,
};

/// Analyzer context that holds all commonly passed parameters for PDA signer account overlap analysis
pub struct PdaSignerAnalyzer {
    pub unsafe_accounts: Vec<UnsafeAccount>,
    pub pda_signers: Vec<PdaSigner>,
    pub def_id: LocalDefId,
}

impl PdaSignerAnalyzer {
    /// Create a new analyzer instance
    pub fn new(
        unsafe_accounts: Vec<UnsafeAccount>,
        pda_signers: Vec<PdaSigner>,
        def_id: LocalDefId,
    ) -> Self {
        Self {
            unsafe_accounts,
            pda_signers,
            def_id,
        }
    }

    /// Analyze basic blocks for CPI calls with unsafe accounts and PDA signers
    pub fn analyze_basic_blocks<'cx, 'tcx>(
        &self,
        mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
        anchor_context_info: &AnchorContextInfo<'tcx>,
    ) {
        for (_bb, bbdata) in mir_analyzer.mir.basic_blocks.iter_enumerated() {
            if let TerminatorKind::Call {
                func: Operand::Constant(func),
                args,
                fn_span,
                ..
            } = &bbdata.terminator().kind
                && let rustc_ty::FnDef(fn_def_id, _) = func.ty().kind()
            {
                let fn_sig = mir_analyzer.cx.tcx.fn_sig(*fn_def_id).skip_binder();
                let return_ty = fn_sig.skip_binder().output();

                if is_anchor_cpi_context(mir_analyzer.cx.tcx, return_ty)
                    && check_cpi_call_is_new_with_signer(mir_analyzer, args, *fn_def_id)
                {
                    // Extract accounts passed to the CPI call
                    let accounts_passed = extract_accounts_passed_to_cpi(
                        args,
                        mir_analyzer,
                        Some(anchor_context_info),
                    );
                    // Check if the CPI call uses a PDA signer
                    let uses_pda_signer =
                        check_cpi_uses_pda_signer(mir_analyzer.cx, *fn_def_id, args, mir_analyzer);

                    // Check for unsafe account overlap
                    if uses_pda_signer && !accounts_passed.is_empty() {
                        self.check_and_report_unsafe_account_overlap(
                            mir_analyzer,
                            fn_span,
                            &accounts_passed,
                        );
                    }
                }
                // Check nested function calls
                else {
                    self.check_and_analyze_nested_function(
                        mir_analyzer,
                        args,
                        *fn_def_id,
                        anchor_context_info,
                    );
                }
            }
        }
    }

    /// Check for unsafe account overlap and report lint violations
    fn check_and_report_unsafe_account_overlap(
        &self,
        mir_analyzer: &MirAnalyzer,
        fn_span: &Span,
        accounts_passed: &std::collections::HashSet<String>,
    ) {
        for unsafe_account in &self.unsafe_accounts {
            // Check if the unsafe account is passed to the CPI call and is mutable
            if accounts_passed.contains(&unsafe_account.account_name) && unsafe_account.is_mutable {
                for pda_signer in &self.pda_signers {
                    if accounts_passed.contains(&pda_signer.account_name)
                        && !has_constraint_preventing_overlap(
                            &unsafe_account.constraints,
                            &unsafe_account.account_name,
                            &pda_signer.account_name,
                        )
                    {
                        let nested_help_msg = format!(
                            "Account `{}` is user-controlled and passed to CPI with PDA `{}` as signer, please verify on the callee side if the account is expected to be uninitialized",
                            unsafe_account.account_name, pda_signer.account_name,
                        );
                        span_lint_and_help(
                            mir_analyzer.cx,
                            PDA_SIGNER_ACCOUNT_OVERLAP,
                            *fn_span,
                            "user-controlled account passed to CPI with PDA signer",
                            Some(unsafe_account.account_span),
                            nested_help_msg,
                        );
                        // Report only the first overlap per unsafe account to avoid duplicate warnings
                        // Multiple PDA signers would produce redundant messages for the same vulnerability
                        break;
                    }
                }
            }
        }
    }

    /// Check and analyze nested function calls
    fn check_and_analyze_nested_function<'cx, 'tcx>(
        &self,
        mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
        args: &[Spanned<Operand<'tcx>>],
        fn_def_id: rustc_hir::def_id::DefId,
        anchor_context_info: &AnchorContextInfo<'tcx>,
    ) {
        let fn_crate_name = mir_analyzer.cx.tcx.crate_name(fn_def_id.krate).to_string();
        let current_crate = mir_analyzer
            .cx
            .tcx
            .crate_name(self.def_id.to_def_id().krate)
            .to_string();
        if fn_crate_name == current_crate {
            // Check if the function is an implementation method
            let is_impl_method =
                is_implementation_method(mir_analyzer.mir, args, anchor_context_info);

            if mir_analyzer
                .get_nested_fn_arguments(args, Some(anchor_context_info))
                .is_some()
                || is_impl_method
            {
                analyze_nested_function_if_available(
                    mir_analyzer.cx,
                    fn_def_id.expect_local(),
                    &self.unsafe_accounts,
                    &self.pda_signers,
                    anchor_context_info,
                );
            }
        }
    }
}

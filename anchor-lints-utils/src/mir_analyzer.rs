use clippy_utils::{source::HasSession, ty::is_type_diagnostic_item};
use rustc_data_structures::graph::dominators::Dominators;
use rustc_hir::{Body as HirBody, def_id::LocalDefId};
use rustc_lint::LateContext;
use rustc_middle::{
    mir::{BasicBlock, Body as MirBody, HasLocalDecls, Local, Operand},
    ty::{self as rustc_ty, Ty, TyKind},
};
use rustc_span::{Span, source_map::Spanned, sym};

use std::collections::{HashMap, HashSet};

use crate::{diag_items::DiagnoticItem, models::*, utils::*};

#[derive(Debug, Clone)]
pub struct AnchorContextInfo<'tcx> {
    pub anchor_context_name: String,
    pub anchor_context_type: Ty<'tcx>,
    pub anchor_context_account_type: Ty<'tcx>,
    pub anchor_context_arg_accounts_type: HashMap<String, Ty<'tcx>>,
    #[allow(unused)]
    pub anchor_context_arg_local: Local,
}

/// Main analyzer struct that encapsulates common MIR analysis state and utilities
pub struct MirAnalyzer<'cx, 'tcx> {
    pub cx: &'cx LateContext<'tcx>,
    pub mir: &'cx MirBody<'tcx>,

    // Assignment maps
    pub assignment_map: HashMap<Local, AssignmentKind<'tcx>>,
    pub cpi_account_local_map: HashMap<Local, Vec<Local>>,
    pub reverse_assignment_map: HashMap<Local, Vec<Local>>,
    pub transitive_assignment_reverse_map: HashMap<Local, Vec<Local>>,
    pub method_call_receiver_map: HashMap<Local, Local>,

    pub dominators: Dominators<BasicBlock>,

    // Optional anchor context info (if function takes Anchor context)
    pub anchor_context_info: Option<AnchorContextInfo<'tcx>>,

    pub param_info: Vec<ParamInfo<'tcx>>,
}

impl<'cx, 'tcx> MirAnalyzer<'cx, 'tcx> {
    // ============================================================================
    // INITIALIZATION & SETUP
    // ============================================================================

    /// Create a new MirAnalyzer with all common initialization
    pub fn new(cx: &'cx LateContext<'tcx>, body: &'cx HirBody<'tcx>, def_id: LocalDefId) -> Self {
        // Get MIR
        let mir = cx.tcx.optimized_mir(def_id.to_def_id());

        // Build assignment maps
        let mir_analysis_maps = build_mir_analysis_maps(mir);
        let transitive_assignment_reverse_map =
            build_transitive_reverse_map(&mir_analysis_maps.reverse_assignment_map);
        let method_call_receiver_map = build_method_call_receiver_map(mir);

        let dominators = mir.basic_blocks.dominators();

        // Get anchor context info (optional - some lints may not need it)
        let anchor_context_info = get_anchor_context_accounts(cx, mir, body);

        Self {
            cx,
            mir,
            assignment_map: mir_analysis_maps.assignment_map,
            reverse_assignment_map: mir_analysis_maps.reverse_assignment_map,
            cpi_account_local_map: mir_analysis_maps.cpi_account_local_map,
            transitive_assignment_reverse_map,
            method_call_receiver_map,
            anchor_context_info,
            dominators: dominators.clone(),
            param_info: get_param_info(cx, mir, body),
        }
    }

    // Updates the anchor context info with the accounts
    pub fn update_anchor_context_info_with_context_accounts(&mut self, body: &HirBody<'tcx>) {
        let context_accounts = get_context_accounts(self.cx, self.mir, body);
        if let Some(context_accounts) = context_accounts {
            self.anchor_context_info = Some(context_accounts);
        }
    }

    // ============================================================================
    // LOCAL VARIABLE RESOLUTION
    // ============================================================================

    // Resolves a local to its original source through assignment chain
    pub fn resolve_to_original_local(
        &self,
        from_local: Local,
        visited: &mut HashSet<Local>,
    ) -> Local {
        if visited.contains(&from_local) {
            return from_local;
        }
        visited.insert(from_local);

        for (src_local, dest_locals) in &self.transitive_assignment_reverse_map {
            if dest_locals.contains(&from_local) {
                return self.resolve_to_original_local(*src_local, visited);
            }
        }

        from_local
    }

    /// Get local from operand
    pub fn get_local_from_operand(
        &self,
        operand: Option<&Spanned<Operand<'tcx>>>,
    ) -> Option<Local> {
        operand.and_then(|op| match &op.node {
            Operand::Copy(place) | Operand::Move(place) => place.as_local(),
            Operand::Constant(_) => None,
        })
    }

    /// Get span from local
    fn get_span_from_local(&self, local: &Local) -> Option<Span> {
        self.mir
            .local_decls()
            .get(*local)
            .map(|d| d.source_info.span)
    }

    // ============================================================================
    // TYPE CHECKING & OPERAND ANALYSIS
    // ============================================================================

    /// Check if a local is a Pubkey type
    pub fn is_pubkey_type(&self, local: Local) -> bool {
        if let Some(decl) = self.mir.local_decls().get(local) {
            let ty = decl.ty.peel_refs();
            if let rustc_middle::ty::TyKind::Adt(adt_def, _) = ty.kind() {
                let def_path = self.cx.tcx.def_path_str(adt_def.did());
                return def_path.contains("Pubkey");
            }
        }
        false
    }

    // Helper to check if a type is AccountInfo
    fn is_account_info_type(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        if let rustc_middle::ty::TyKind::Adt(adt_def, _) = ty.kind() {
            let def_path = self.cx.tcx.def_path_str(adt_def.did());
            return def_path.starts_with("anchor_lang::prelude::")
                || def_path == "solana_program::account_info::AccountInfo";
        }
        false
    }

    /// Get origin of an operand (Constant, Parameter, or Unknown)
    pub fn origin_of_operand(&self, op: &Operand<'tcx>) -> Origin {
        match op {
            Operand::Constant(_) => Origin::Constant,
            Operand::Copy(place) | Operand::Move(place) => {
                if let Some(local) = place.as_local() {
                    self.resolve_local_origin(local)
                } else {
                    Origin::Unknown
                }
            }
        }
    }

    /// Resolve the origin of a local variable
    fn resolve_local_origin(&self, local: Local) -> Origin {
        // Check if it's a function parameter
        if local.index() < self.mir.arg_count {
            return Origin::Parameter;
        }

        // Check assignment map
        if let Some(kind) = self.assignment_map.get(&local) {
            match kind {
                AssignmentKind::Const => return Origin::Constant,
                AssignmentKind::FromPlace(src_place) => {
                    if let Some(src_local) = src_place.as_local() {
                        return self.resolve_local_origin(src_local);
                    }
                }
                _ => {}
            }
        }
        Origin::Unknown
    }

    /// If this [`Operand`] refers to a [`Local`] that is a `Pubkey`, return it
    pub fn pubkey_operand_to_local(&self, op: &Operand<'_>) -> Option<Local> {
        match op {
            Operand::Copy(place) | Operand::Move(place) => {
                place.as_local().filter(|local| self.is_pubkey_type(*local))
            }
            Operand::Constant(_) => None,
        }
    }

    /// If these function args are two `Pubkey` references, return the corresponding
    /// [`Local`]s.
    pub fn args_as_pubkey_locals(&self, args: &[Spanned<Operand>]) -> Option<(Local, Local)> {
        Option::zip(
            self.pubkey_operand_to_local(&args.first()?.node),
            self.pubkey_operand_to_local(&args.get(1)?.node),
        )
    }

    // ============================================================================
    // CPI CONTEXT ANALYSIS
    // ============================================================================

    pub fn is_from_cpi_context(&self, raw_local: Local) -> Option<CpiAccountInfo> {
        if let Some(anchor_context_info) = &self.anchor_context_info {
            let local = self.resolve_to_original_local(raw_local, &mut HashSet::new());

            let local_decl = self.mir.local_decls().get(local)?;
            let local_ty = local_decl.ty.peel_refs();
            let span = local_decl.source_info.span;

            // First, match by type against known accounts
            let mut matching_accounts: Vec<(&String, &rustc_ty::Ty<'tcx>)> = anchor_context_info
                .anchor_context_arg_accounts_type
                .iter()
                .filter(|(_, account_ty)| {
                    let account_ty_peeled = account_ty.peel_refs();
                    match (local_ty.kind(), account_ty_peeled.kind()) {
                        (TyKind::Adt(local_adt, _), TyKind::Adt(account_adt, _)) => {
                            local_adt.did() == account_adt.did()
                        }
                        _ => local_ty == account_ty_peeled,
                    }
                })
                .collect();

            if matching_accounts.len() == 1 {
                let (account_name, _) = matching_accounts[0];
                return Some(CpiAccountInfo {
                    account_name: account_name.clone(),
                    account_local: anchor_context_info.anchor_context_arg_local,
                });
            }

            if matching_accounts.is_empty() {
                matching_accounts = anchor_context_info
                    .anchor_context_arg_accounts_type
                    .iter()
                    .collect();
            }

            // Multiple matches — try to disambiguate using the span text (ctx.accounts.<name>)
            if let Ok(snippet) = self.cx.sess().source_map().span_to_snippet(span) {
                let cleaned_snippet = remove_comments(&snippet);

                // Helper function to find account by name
                let find_account_by_name = |name: &str| -> Option<String> {
                    matching_accounts
                        .iter()
                        .find(|(account_name, _)| account_name.as_str() == name)
                        .map(|(account_name, _)| (*account_name).clone())
                };

                // Try to extract account name from snippet patterns
                let account_name =
                    if let Some(after_accounts) = cleaned_snippet.split(".accounts.").nth(1) {
                        // Pattern: ctx.accounts.<name>
                        after_accounts
                            .split('.')
                            .next()
                            .and_then(|s| find_account_by_name(s.trim()))
                    } else if cleaned_snippet
                        .starts_with(anchor_context_info.anchor_context_name.as_str())
                    {
                        // Pattern: ctx.<name> (after removing ctx prefix)
                        let remaining = cleaned_snippet
                            .replace(anchor_context_info.anchor_context_name.as_str(), "");
                        remaining
                            .split('.')
                            .nth(1)
                            .and_then(|s| find_account_by_name(s.trim()))
                    } else {
                        None
                    };

                if let Some(account_name) = account_name {
                    return Some(CpiAccountInfo {
                        account_name,
                        account_local: anchor_context_info.anchor_context_arg_local,
                    });
                }
            }
        }
        None
    }

    pub fn check_cpi_context_variables_are_same(
        &self,
        from: &Local,
        to: &Local,
        visited: &mut HashSet<Local>,
    ) -> bool {
        if visited.contains(from) {
            return false;
        }
        visited.insert(*from);
        if to == from {
            return true;
        }
        if let Some(assignment_locals) = &self.transitive_assignment_reverse_map.get(from) {
            for assignment_local in assignment_locals.iter() {
                if self.check_cpi_context_variables_are_same(assignment_local, to, visited) {
                    return true;
                }
            }
            return false;
        }
        false
    }

    pub fn takes_cpi_context(&self, args: &[Spanned<Operand>]) -> bool {
        args.iter().any(|arg| {
            if let Operand::Copy(place) | Operand::Move(place) = &arg.node
                && let Some(local) = place.as_local()
                && let Some(decl) = self.mir.local_decls().get(local)
            {
                DiagnoticItem::AnchorCpiContext.defid_is_type(self.cx.tcx, decl.ty.peel_refs())
            } else {
                false
            }
        })
    }

    /// Check if two locals come from the same CPI context account
    pub fn are_same_account(&self, local1: Local, local2: Local) -> bool {
        if let (Some(account1), Some(account2)) = (
            self.is_from_cpi_context(local1),
            self.is_from_cpi_context(local2),
        ) {
            account1.account_name == account2.account_name
        } else {
            false
        }
    }

    // Finds the accounts struct in a CPI context.
    pub fn find_cpi_accounts_struct(
        &self,
        account_stuct_local: &Local,
        visited: &mut HashSet<Local>,
    ) -> Option<Vec<Local>> {
        if let Some(accounts) = self.cpi_account_local_map.get(account_stuct_local) {
            return Some(accounts.clone());
        }
        if visited.contains(account_stuct_local) {
            return None;
        }
        visited.insert(*account_stuct_local);
        for (lhs, rhs) in &self.reverse_assignment_map {
            if rhs.contains(account_stuct_local) {
                // recursively check the lhs
                if let Some(accounts) = self.find_cpi_accounts_struct(lhs, visited) {
                    return Some(accounts);
                }
            }
        }
        None
    }

    // ============================================================================
    // NESTED FUNCTION ARGUMENT EXTRACTION
    // ============================================================================

    // Helper to extract (local, account_ty) from an operand
    fn extract_local_and_ty_from_operand(
        &self,
        arg: &Spanned<Operand<'tcx>>,
    ) -> Option<(Local, rustc_ty::Ty<'tcx>)> {
        if let Operand::Move(place) | Operand::Copy(place) = &arg.node
            && let Some(local) = place.as_local()
            && let Some(account_ty) = self.mir.local_decls().get(local).map(|d| d.ty.peel_refs())
        {
            Some((local, account_ty))
        } else {
            None
        }
    }

    // Helper to create NestedAccount from account_ty and arg_index
    fn create_nested_account(
        account_ty: rustc_ty::Ty<'tcx>,
        arg_index: usize,
    ) -> NestedAccount<'tcx> {
        NestedAccount {
            account_ty,
            account_local: Local::from_usize(arg_index + 1),
        }
    }

    // Helper to extract account name from span, with fallback
    fn extract_account_name_from_span(
        &self,
        arg: &Spanned<Operand<'tcx>>,
        fallback_name: Option<&String>,
    ) -> Option<String> {
        if let Ok(snippet) = self.cx.sess().source_map().span_to_snippet(arg.span) {
            let cleaned_snippet = remove_comments(&snippet);
            if let Some(acc_name) = extract_account_name_from_string(&cleaned_snippet) {
                return Some(acc_name);
            }
        }
        fallback_name.cloned()
    }

    // Extracts argumments if they contains context/context.accounts/context.accounts.account as arguments
    pub fn get_nested_fn_arguments(
        &self,
        args: &[Spanned<Operand<'tcx>>],
        anchor_context_info: Option<&AnchorContextInfo<'tcx>>,
    ) -> Option<NestedArgument<'tcx>> {
        let mut nested_argument = NestedArgument {
            arg_type: NestedArgumentType::Ctx,
            accounts: HashMap::new(),
        };
        let mut found = false;
        let cpi_context_info = anchor_context_info.or(self.anchor_context_info.as_ref());

        for (arg_index, arg) in args.iter().enumerate() {
            let Some((_local, account_ty)) = self.extract_local_and_ty_from_operand(arg) else {
                continue;
            };

            let Some(cpi_context_info) = cpi_context_info else {
                continue;
            };

            if account_ty == cpi_context_info.anchor_context_type {
                nested_argument.arg_type = NestedArgumentType::Ctx;
                found = true;
                break;
            } else if account_ty == cpi_context_info.anchor_context_account_type {
                nested_argument.arg_type = NestedArgumentType::Accounts;
                found = true;
                break;
            } else if let Some((account_name, _)) = cpi_context_info
                .anchor_context_arg_accounts_type
                .iter()
                .find(|(_, accty)| *accty == &account_ty || self.is_account_info_type(account_ty))
            {
                if let Some(acc_name) = self.extract_account_name_from_span(arg, Some(account_name))
                {
                    nested_argument
                        .accounts
                        .insert(acc_name, Self::create_nested_account(account_ty, arg_index));
                }
                nested_argument.arg_type = NestedArgumentType::Account;
                found = true;
            }
        }

        if found { Some(nested_argument) } else { None }
    }

    // Extracts arguments if they contain context/context.accounts/context.accounts.account as arguments
    pub fn get_nested_fn_arguments_as_params(
        &self,
        args: &[Spanned<Operand<'tcx>>],
    ) -> Option<NestedArgument<'tcx>> {
        let mut nested_argument = NestedArgument {
            arg_type: NestedArgumentType::Account,
            accounts: HashMap::new(),
        };
        let mut found = false;

        for (arg_index, arg) in args.iter().enumerate() {
            let Some((local, account_ty)) = self.extract_local_and_ty_from_operand(arg) else {
                continue;
            };

            if self.is_account_info_type(account_ty)
                && let Some(param) = self.check_local_is_param(local)
            {
                nested_argument.accounts.insert(
                    param.param_name.clone(),
                    Self::create_nested_account(account_ty, arg_index),
                );
                found = true;
            }
        }

        if found { Some(nested_argument) } else { None }
    }

    // ============================================================================
    // VECTOR & ACCOUNT EXTRACTION
    // ============================================================================

    // Collects the accounts from the account_infos argument.
    pub fn collect_accounts_from_account_infos_arg(
        &self,
        arg: &Spanned<Operand<'tcx>>,
        return_only_name: bool,
    ) -> Vec<AccountNameAndLocal> {
        if let Operand::Copy(place) | Operand::Move(place) = arg.node
            && let Some(vec_local) = place.as_local()
            && let Some(vec_ty) = self
                .mir
                .local_decls()
                .get(vec_local)
                .map(|d| d.ty.peel_refs())
            && (is_type_diagnostic_item(self.cx, vec_ty, sym::Vec)
                || matches!(vec_ty.kind(), TyKind::Slice(_)))
        {
            return self.get_vec_elements(&vec_local, &mut HashSet::new(), return_only_name);
        }
        Vec::new()
    }

    pub fn get_vec_elements(
        &self,
        local: &Local,
        visited_locals: &mut HashSet<Local>,
        // reverse_assignment_map: &HashMap<Local, Vec<Local>>,
        // method_call_receiver_map: &HashMap<Local, Local>,
        return_only_name: bool,
    ) -> Vec<AccountNameAndLocal> {
        let mut elements = Vec::new();
        if let Some(span) = self.get_span_from_local(local) {
            if visited_locals.contains(local) {
                if let Some(method_call_receiver) = self.method_call_receiver_map.get(local) {
                    return self.get_vec_elements(
                        method_call_receiver,
                        visited_locals,
                        return_only_name,
                    );
                }
                return elements;
            }
            visited_locals.insert(*local);
            let mut cleaned_snippet = String::new();
            if let Some(full_vec) = extract_vec_snippet_from_span(self.cx, span) {
                cleaned_snippet = remove_comments(&full_vec);
            } else if let Ok(snippet) = self.cx.tcx.sess().source_map().span_to_snippet(span) {
                cleaned_snippet = remove_comments(&snippet);
            }
            for element in extract_vec_elements(&cleaned_snippet) {
                if let Some(account_name) = extract_context_account(&element, return_only_name) {
                    elements.push(AccountNameAndLocal {
                        account_name,
                        account_local: *local,
                    });
                }
            }
            if !elements.is_empty() {
                return elements;
            }
            let resolved_local = self.resolve_to_original_local(*local, &mut HashSet::new());
            return self.get_vec_elements(&resolved_local, visited_locals, return_only_name);
        }

        elements
    }

    // Checks if a local is an account name and returns the account name and local.
    pub fn check_local_and_assignment_locals(
        &self,
        account_local: &Local,
        visited: &mut HashSet<Local>,
        return_only_name: bool,
        maybe_account_name: &mut String,
    ) -> Vec<AccountNameAndLocal> {
        let local_decl = &self.mir.local_decls[*account_local];
        let span = local_decl.source_info.span;
        let mut results = Vec::new();
        if let Ok(snippet) = self.cx.sess().source_map().span_to_snippet(span) {
            let cleaned_snippet = remove_comments(&snippet);
            if cleaned_snippet.trim_start().contains("vec!") {
                for element in extract_vec_elements(&cleaned_snippet) {
                    if let Some(account_name) = extract_context_account(&element, return_only_name)
                    {
                        results.push(AccountNameAndLocal {
                            account_name,
                            account_local: *account_local,
                        });
                    }
                }
                return results;
            }
            if let Some(account_name) = extract_context_account(&cleaned_snippet, return_only_name)
            {
                if cleaned_snippet.contains("accounts.") {
                    results.push(AccountNameAndLocal {
                        account_name,
                        account_local: *account_local,
                    });
                    return results;
                }
                *maybe_account_name = account_name;
            }
            if let Ok(file_span) = self.cx.sess().source_map().span_to_lines(span) {
                let file = &file_span.file;
                let start_line_idx = file_span.lines[0].line_index;
                if let Some(src) = file.src.as_ref() {
                    let lines: Vec<&str> = src.lines().collect();
                    if let Some(account_name) =
                        extract_context_account(lines[start_line_idx], return_only_name)
                    {
                        if lines[start_line_idx].contains("accounts.") {
                            results.push(AccountNameAndLocal {
                                account_name,
                                account_local: *account_local,
                            });
                            return results;
                        }
                        *maybe_account_name = account_name;
                    }
                }
            }
        }
        if visited.contains(account_local) {
            if !maybe_account_name.is_empty() && return_only_name {
                results.push(AccountNameAndLocal {
                    account_name: maybe_account_name.clone(),
                    account_local: *account_local,
                });
                return results;
            }
            return results;
        }
        visited.insert(*account_local);

        // First, check if this is a method call result
        if let Some(receiver_local) = self.method_call_receiver_map.get(account_local)
            && let account_name_and_locals = self.check_local_and_assignment_locals(
                receiver_local,
                visited,
                return_only_name,
                maybe_account_name,
            )
            && !account_name_and_locals.is_empty()
        {
            return account_name_and_locals;
        }

        // Then check assignment map (for regular assignments like _4 = _3)
        for (lhs, rhs) in &self.transitive_assignment_reverse_map {
            if rhs.contains(account_local)
                && let account_name_and_locals = self.check_local_and_assignment_locals(
                    lhs,
                    visited,
                    return_only_name,
                    maybe_account_name,
                )
                && !account_name_and_locals.is_empty()
            {
                return account_name_and_locals;
            }
        }
        if !maybe_account_name.is_empty() && return_only_name {
            results.push(AccountNameAndLocal {
                account_name: maybe_account_name.clone(),
                account_local: *account_local,
            });
            return results;
        }
        results
    }

    // ============================================================================
    // PARAMETER ANALYSIS
    // ============================================================================

    pub fn check_local_is_param(&self, local: Local) -> Option<&ParamInfo<'tcx>> {
        let local = self.resolve_to_original_local(local, &mut HashSet::new());
        for param in &self.param_info {
            if param.param_local == local {
                return Some(param);
            }
        }

        if let Some(span) = self.get_span_from_local(&local)
            && let Ok(snippet) = self.cx.sess().source_map().span_to_snippet(span)
        {
            let cleaned_snippet = remove_comments(&snippet);

            // Extract account name from patterns like "program.key()", "account.field", etc.
            let account_name = cleaned_snippet
                .split('.')
                .next()
                .map(|s| {
                    s.trim()
                        .trim_start_matches("&mut ")
                        .trim_start_matches("& ")
                })
                .filter(|s| !s.is_empty());

            if let Some(account_name) = account_name {
                // Check if any parameter matches this account name
                for param in &self.param_info {
                    if param.param_name == account_name {
                        return Some(param);
                    }
                }
            }
        }
        None
    }
}

use clippy_utils::source::HasSession;
use rustc_data_structures::graph::dominators::Dominators;
use rustc_hir::{Body as HirBody, def_id::LocalDefId};
use rustc_lint::LateContext;
use rustc_middle::{
    mir::{BasicBlock, Body as MirBody, HasLocalDecls, Local, Operand},
    ty::{self as rustc_ty, Ty, TyKind},
};
use rustc_span::source_map::Spanned;

use std::collections::{HashMap, HashSet};

use crate::models::{AssignmentKind, CpiAccountInfo, Origin};
use crate::{
    diag_items::DiagnoticItem,
    utils::{
        build_mir_analysis_maps, build_transitive_reverse_map, get_anchor_context_accounts,
        remove_comments,
    },
};

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

    pub dominators: Dominators<BasicBlock>,

    // Optional anchor context info (if function takes Anchor context)
    pub anchor_context_info: Option<AnchorContextInfo<'tcx>>,
}

impl<'cx, 'tcx> MirAnalyzer<'cx, 'tcx> {
    /// Create a new MirAnalyzer with all common initialization
    pub fn new(cx: &'cx LateContext<'tcx>, body: &'cx HirBody<'tcx>, def_id: LocalDefId) -> Self {
        // Get MIR
        let mir = cx.tcx.optimized_mir(def_id.to_def_id());

        // Build assignment maps
        let mir_analysis_maps = build_mir_analysis_maps(mir);
        let transitive_assignment_reverse_map =
            build_transitive_reverse_map(&mir_analysis_maps.reverse_assignment_map);

        let dominators = mir.basic_blocks.dominators();

        // Get anchor context info (optional - some lints may not need it)
        let anchor_context_info = get_anchor_context_accounts(cx, body);

        Self {
            cx,
            mir,
            assignment_map: mir_analysis_maps.assignment_map,
            reverse_assignment_map: mir_analysis_maps.reverse_assignment_map,
            cpi_account_local_map: mir_analysis_maps.cpi_account_local_map,
            transitive_assignment_reverse_map,
            anchor_context_info,
            dominators: dominators.clone(),
        }
    }

    /// Resolve a local to its original source through assignment chain
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
                if let Some(after_accounts) = cleaned_snippet.split(".accounts.").nth(1)
                    && let Some(name) = after_accounts.split('.').next().map(|s| s.trim())
                    && let Some((account_name, _)) = matching_accounts
                        .into_iter()
                        .find(|(account_name, _)| account_name.as_str() == name)
                {
                    return Some(CpiAccountInfo {
                        account_name: account_name.clone(),
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

    /// If these function args are two `Pubkey` references, return the corresponding
    /// [`Local`]s.
    pub fn args_as_pubkey_locals(&self, args: &[Spanned<Operand>]) -> Option<(Local, Local)> {
        Option::zip(
            self.pubkey_operand_to_local(&args.first()?.node),
            self.pubkey_operand_to_local(&args.get(1)?.node),
        )
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
}

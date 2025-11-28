use clippy_utils::source::HasSession;

use rustc_hir::{Body as HirBody, PatKind};
use rustc_lint::LateContext;
use rustc_middle::{
    mir::{Body as MirBody, Local, Operand, Place, Rvalue, StatementKind},
    ty::TyKind,
};

use std::collections::{HashMap, HashSet, VecDeque};

use crate::mir_analyzer::AnchorContextInfo;
use crate::models::{AssignmentKind, MirAnalysisMaps};

pub fn remove_comments(code: &str) -> String {
    code.lines()
        .filter(|line| !line.trim().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Get anchor context accounts from function body
pub fn get_anchor_context_accounts<'tcx>(
    cx: &LateContext<'tcx>,
    body: &HirBody<'tcx>,
) -> Option<AnchorContextInfo<'tcx>> {
    let params = body.params;
    for (param_index, param) in params.iter().enumerate() {
        let param_ty = cx.typeck_results().pat_ty(param.pat).peel_refs();
        if let TyKind::Adt(adt_def, generics) = param_ty.kind() {
            let struct_name = cx.tcx.def_path_str(adt_def.did());
            if struct_name.ends_with("anchor_lang::context::Context")
                || struct_name.ends_with("anchor_lang::prelude::Context")
            {
                let variant = adt_def.non_enum_variant();
                for field in &variant.fields {
                    let field_name = field.ident(cx.tcx).to_string();
                    let field_ty = field.ty(cx.tcx, generics);
                    if field_name == "accounts" {
                        let accounts_struct_ty = field_ty.peel_refs();
                        if let TyKind::Adt(accounts_adt_def, accounts_generics) =
                            accounts_struct_ty.kind()
                        {
                            let accounts_variant = accounts_adt_def.non_enum_variant();
                            let mut cpi_ctx_accounts = HashMap::new();
                            for account_field in &accounts_variant.fields {
                                let account_name = account_field.ident(cx.tcx).to_string();
                                let account_ty = account_field.ty(cx.tcx, accounts_generics);
                                cpi_ctx_accounts.insert(account_name, account_ty);
                            }
                            let param_name = match param.pat.kind {
                                PatKind::Binding(_, _, ident, _) => ident.name.as_str().to_string(),
                                _ => {
                                    // fallback to span
                                    if let Ok(snippet) =
                                        cx.sess().source_map().span_to_snippet(param.pat.span)
                                    {
                                        let cleaned_snippet = remove_comments(&snippet);
                                        cleaned_snippet
                                            .split(':')
                                            .next()
                                            .unwrap_or("_")
                                            .trim()
                                            .to_string()
                                    } else {
                                        format!("param_{}", param_index)
                                    }
                                }
                            };
                            let arg_local = Local::from_usize(param_index + 1);
                            return Some(AnchorContextInfo {
                                anchor_context_name: param_name,
                                anchor_context_account_type: accounts_struct_ty,
                                anchor_context_arg_local: arg_local,
                                anchor_context_type: param_ty,
                                anchor_context_arg_accounts_type: cpi_ctx_accounts,
                            });
                        }
                    }
                }
            }
        }
    }
    None
}

/// Builds the analysis maps for the MIR body
pub fn build_mir_analysis_maps<'tcx>(mir: &MirBody<'tcx>) -> MirAnalysisMaps<'tcx> {
    let mut assignment_map: HashMap<Local, AssignmentKind<'tcx>> = HashMap::new();
    let mut reverse_assignment_map: HashMap<Local, Vec<Local>> = HashMap::new();
    let mut cpi_account_local_map: HashMap<Local, Vec<Local>> = HashMap::new();

    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        for statement in &bbdata.statements {
            if let StatementKind::Assign(box (dest_place, rvalue)) = &statement.kind
                && let Some(dest_local) = dest_place.as_local()
            {
                // 1️⃣ AssignmentKind classification
                let kind = match rvalue {
                    Rvalue::Use(Operand::Constant(_)) => AssignmentKind::Const,
                    Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) => {
                        AssignmentKind::FromPlace(*src)
                    }
                    Rvalue::Ref(_, _, src_place) => AssignmentKind::RefTo(*src_place),
                    _ => AssignmentKind::Other,
                };
                assignment_map.insert(dest_local, kind);

                // Helper closure used for reverse mapping
                let mut record_mapping = |src_place: &Place<'tcx>| {
                    reverse_assignment_map
                        .entry(src_place.local)
                        .or_default()
                        .push(dest_local);
                };

                // 2️⃣ CPI map only for Aggregates
                if let Rvalue::Aggregate(_, field_operands) = rvalue {
                    for operand in field_operands {
                        if let Operand::Copy(field_place) | Operand::Move(field_place) = operand
                            && let Some(field_local) = field_place.as_local()
                        {
                            cpi_account_local_map
                                .entry(dest_local)
                                .or_default()
                                .push(field_local);
                        }
                    }
                }

                // 3️⃣ Reverse mapping for all rvalue types
                match rvalue {
                    Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) => record_mapping(src),
                    Rvalue::Ref(_, _, src) => record_mapping(src),
                    Rvalue::Cast(_, Operand::Copy(src) | Operand::Move(src), _) => {
                        record_mapping(src)
                    }
                    Rvalue::Aggregate(_, operands) => {
                        for operand in operands {
                            if let Operand::Copy(src) | Operand::Move(src) = operand {
                                record_mapping(src);
                            }
                        }
                    }
                    Rvalue::CopyForDeref(src) => record_mapping(src),
                    _ => {}
                }
            }
        }
    }

    MirAnalysisMaps {
        assignment_map,
        reverse_assignment_map,
        cpi_account_local_map,
    }
}

/// Build transitive reverse map from direct reverse map
pub fn build_transitive_reverse_map(
    direct_map: &HashMap<Local, Vec<Local>>,
) -> HashMap<Local, Vec<Local>> {
    let mut transitive_map: HashMap<Local, Vec<Local>> = HashMap::new();

    for (&src, dests) in direct_map {
        let mut visited = HashSet::new();
        let mut queue: VecDeque<Local> = VecDeque::from(dests.clone());

        while let Some(next) = queue.pop_front() {
            if visited.insert(next) {
                transitive_map.entry(src).or_default().push(next);

                if let Some(next_dests) = direct_map.get(&next) {
                    for &nd in next_dests {
                        queue.push_back(nd);
                    }
                }
            }
        }
    }

    for vec in transitive_map.values_mut() {
        vec.sort();
    }

    transitive_map
}

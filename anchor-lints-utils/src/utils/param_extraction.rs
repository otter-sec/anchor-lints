use clippy_utils::source::HasSession;

use rustc_hir::{Body as HirBody, PatKind};
use rustc_lint::LateContext;
use rustc_middle::{
    mir::{Body as MirBody, HasLocalDecls, Local, Operand},
    ty::TyKind,
};
use rustc_span::source_map::Spanned;

use super::string_extraction::remove_comments;
use crate::models::*;

/// Extract parameter data from a HIR parameter
pub(crate) fn extract_param_data<'tcx>(
    cx: &LateContext<'tcx>,
    mir: &MirBody<'tcx>,
    param_index: usize,
    param: &rustc_hir::Param<'tcx>,
) -> Option<ParamData<'tcx>> {
    let param_local = Local::from_usize(param_index + 1);
    let param_ty = mir.local_decls().get(param_local)?.ty.peel_refs();
    let param_name = extract_param_name(cx, param, param_index);

    let (adt_def, struct_name) = if let TyKind::Adt(adt_def, generics) = param_ty.kind() {
        let struct_name = cx.tcx.def_path_str(adt_def.did());
        (Some((adt_def, generics)), Some(struct_name))
    } else {
        (None, None)
    };

    Some(ParamData {
        param_index,
        param_local,
        param_name,
        param_ty,
        adt_def,
        struct_name,
    })
}

/// Extract parameter name from HIR parameter
fn extract_param_name<'tcx>(
    cx: &LateContext<'tcx>,
    param: &rustc_hir::Param<'tcx>,
    param_index: usize,
) -> String {
    match param.pat.kind {
        PatKind::Binding(_, _, ident, _) => ident.name.as_str().to_string(),
        _ => {
            // fallback to span
            if let Ok(snippet) = cx.sess().source_map().span_to_snippet(param.pat.span) {
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
    }
}

/// Get param info from function body
pub fn get_param_info<'tcx>(
    cx: &LateContext<'tcx>,
    mir: &MirBody<'tcx>,
    body: &HirBody<'tcx>,
) -> Vec<ParamInfo<'tcx>> {
    if body.params.is_empty() {
        return Vec::new();
    }

    let mut param_info: Vec<ParamInfo<'tcx>> = Vec::new();

    for (param_index, param) in body.params.iter().enumerate() {
        let Some(param_data) = extract_param_data(cx, mir, param_index, param) else {
            continue;
        };

        if let Some(struct_name) = param_data.struct_name {
            // Only collect single Anchor account types
            if is_single_anchor_account_type(&struct_name) {
                param_info.push(ParamInfo {
                    param_index,
                    param_name: param_data.param_name,
                    param_local: param_data.param_local,
                    param_ty: param_data.param_ty,
                });
            }
        }
    }

    param_info
}

/// Check if a type is a single Anchor account type (not an accounts struct)
pub(crate) fn is_single_anchor_account_type(struct_name: &str) -> bool {
    // Exclude single account types
    struct_name.starts_with("anchor_lang::prelude::")
        || struct_name == "solana_program::account_info::AccountInfo"
}

// Extract the local from the argument at the given index
pub fn extract_arg_local(args: &[Spanned<Operand>], index: usize) -> Option<Local> {
    if let Some(cpi_ctx_arg) = args.get(index)
        && let Operand::Copy(place) | Operand::Move(place) = &cpi_ctx_arg.node
        && let Some(arg_local) = place.as_local()
    {
        return Some(arg_local);
    }
    None
}

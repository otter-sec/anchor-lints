use rustc_lint::LateContext;
use rustc_middle::ty::{Ty, TyKind};

/// Check if a type is Option<UncheckedAccount>
pub fn is_option_unchecked_account_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, substs) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        if (def_path == "core::option::Option" || def_path == "std::option::Option")
            && let Some(inner_ty) = substs.types().next()
        {
            return is_unchecked_account_type(cx, inner_ty);
        }
    }
    false
}

/// Check if a type is UncheckedAccount
pub fn is_unchecked_account_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty) -> bool {
    let ty = ty.peel_refs();
    if let TyKind::Adt(adt_def, _) = ty.kind() {
        let def_path = cx.tcx.def_path_str(adt_def.did());
        return def_path == "anchor_lang::prelude::UncheckedAccount";
    }
    false
}

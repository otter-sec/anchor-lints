use rustc_lint::LateContext;
use rustc_middle::ty::{Ty, TyKind};

use crate::utils::types::AccountField;

/// Extract all fields from an account struct type.
pub fn extract_inner_struct_fields<'tcx>(
    cx: &LateContext<'tcx>,
    inner_ty: Ty<'tcx>,
) -> Option<Vec<AccountField<'tcx>>> {
    let ty = inner_ty.peel_refs();
    if let TyKind::Adt(adt_def, generics) = ty.kind() {
        if !adt_def.is_struct() && !adt_def.is_union() {
            return None;
        }
        let variant = adt_def.non_enum_variant();
        let mut fields = Vec::new();
        for field in &variant.fields {
            let name = field.ident(cx.tcx).to_string();
            let f_ty = field.ty(cx.tcx, generics);
            fields.push(AccountField { name, ty: f_ty });
        }
        Some(fields)
    } else {
        None
    }
}

/// Determine if a field should be ignored when checking for initialization.
pub fn should_ignore_field<'tcx>(cx: &LateContext<'tcx>, field: &AccountField<'tcx>) -> bool {
    let n = field.name.as_str();

    // Check for padding/reserved field name patterns
    if n.starts_with("padding")
        || n.starts_with("reserved")
        || n == "padding"
        || n == "reserved"
        || n.starts_with('_')
        // TokenAccounts/Mints etc. will get "0" as name, so skip them
        || n == "0"
    {
        return true;
    }

    // Check if the field type is a primitive
    is_primitive_type(cx, field.ty)
}

/// Check if a type is a primitive type (int, uint, bool, float, char) or array of primitives.
fn is_primitive_type<'tcx>(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
    let ty = ty.peel_refs();
    match ty.kind() {
        TyKind::Int(_) | TyKind::Uint(_) | TyKind::Bool | TyKind::Float(_) | TyKind::Char => true,
        TyKind::Array(elem_ty, _) => {
            // Arrays of primitives (like [u8; N] for buffers) are also safe
            is_primitive_type(cx, *elem_ty)
        }
        TyKind::Adt(adt_def, generics) => {
            let def_path = cx.tcx.def_path_str(adt_def.did());

            // Exclude known semantic types that should be checked
            if def_path.ends_with("::Pubkey") || def_path.ends_with("::Signer")
            {
                return false;
            }
            // Check if it's a struct with only primitive fields
            if adt_def.is_struct() {
                let variant = adt_def.non_enum_variant();
                // Check all fields are primitives
                variant.fields.iter().all(|field| {
                    let field_ty = field.ty(cx.tcx, generics);
                    is_primitive_type(cx, field_ty)
                })
            } else {
                false
            }
        }
        _ => false,
    }
}

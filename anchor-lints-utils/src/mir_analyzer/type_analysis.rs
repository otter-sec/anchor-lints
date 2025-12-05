use rustc_middle::{
    mir::{HasLocalDecls, Local, Operand},
    ty::Ty,
};
use rustc_span::source_map::Spanned;

use super::types::MirAnalyzer;
use crate::models::*;

impl<'cx, 'tcx> MirAnalyzer<'cx, 'tcx> {
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
    pub(crate) fn is_account_info_type(&self, ty: Ty<'tcx>) -> bool {
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
}

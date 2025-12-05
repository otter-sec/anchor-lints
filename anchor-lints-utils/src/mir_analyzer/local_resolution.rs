use rustc_middle::{
    mir::{HasLocalDecls, Local, Operand, Place},
    ty::Ty,
};
use rustc_span::{Span, source_map::Spanned};

use std::collections::HashSet;

use super::types::MirAnalyzer;

impl<'cx, 'tcx> MirAnalyzer<'cx, 'tcx> {
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
    pub(crate) fn get_span_from_local(&self, local: &Local) -> Option<Span> {
        self.mir
            .local_decls()
            .get(*local)
            .map(|d| d.source_info.span)
    }

    /// Get ty from operand
    pub fn get_ty_from_operand(&self, operand: &Operand<'tcx>) -> Option<Ty<'tcx>> {
        match operand {
            Operand::Constant(c) => Some(c.ty()),
            Operand::Copy(place) | Operand::Move(place) => self.get_ty_from_place(place),
        }
    }

    /// Get ty from place
    pub fn get_ty_from_place(&self, place: &Place<'tcx>) -> Option<Ty<'tcx>> {
        if let Some(local) = place.as_local()
            && let Some(decl) = self.mir.local_decls().get(local)
        {
            return Some(decl.ty.peel_refs());
        }
        None
    }
}

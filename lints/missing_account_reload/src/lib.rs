#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use std::collections::{HashMap, HashSet, VecDeque};

use clippy_utils::diagnostics::span_lint_and_note;
use clippy_utils::fn_has_unsatisfiable_preds;
use clippy_utils::source::HasSession;
use clippy_utils::ty::is_type_diagnostic_item;
use rustc_hir::{Body as HirBody, FnDecl, def_id::LocalDefId, intravisit::FnKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::mir::{
    BasicBlock, BasicBlocks, Body as MirBody, HasLocalDecls, Local, Operand, Place, Rvalue,
    StatementKind, TerminatorKind,
};
use rustc_middle::ty::{self as rustc_ty};
use rustc_span::source_map::Spanned;
use rustc_span::{Span, Symbol};

dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Identifies access of an account without calling `reload()` after a CPI.
    ///
    /// ### Why is this bad?
    /// After a CPI, deserialized accounts do not have their data updated automatically.
    /// Accessing them without calling `reload` may lead to stale data being loaded.
    /// ```
    pub MISSING_ACCOUNT_RELOAD,
    Warn,
    "account accessed after a CPI without reloading",
    MissingAccountReload::default()
}

#[derive(Default)]
pub struct MissingAccountReload;

impl<'tcx> LateLintPass<'tcx> for MissingAccountReload {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        _kind: FnKind<'tcx>,
        _: &FnDecl<'tcx>,
        _: &HirBody<'tcx>,
        fn_span: Span,
        def_id: LocalDefId,
    ) {
        // skip macro expansions
        if fn_span.from_expansion() {
            return;
        }
        // Building MIR for `fn`s with unsatisfiable preds results in ICE.
        if fn_has_unsatisfiable_preds(cx, def_id.to_def_id()) {
            return;
        }

        let account_reload_sym = Symbol::intern("AnchorAccountReload");
        let deref_method_sym = Symbol::intern("deref_method");
        let cpi_invoke_syms = [
            Symbol::intern("AnchorCpiInvoke"),
            Symbol::intern("AnchorCpiInvokeUnchecked"),
            Symbol::intern("AnchorCpiInvokeSigned"),
            Symbol::intern("AnchorCpiInvokeSignedUnchecked"),
        ];
        let anchor_cpi_sym = Symbol::intern("AnchorCpiContext");

        let mir = cx.tcx.optimized_mir(def_id.to_def_id());

        // We need to identify
        // A) CPI invocations
        // Then, for each account
        // B) Account data accesses (i.e. a call to `Deref` on `Account.name`)
        // C) Account reloads (i.e. a call to `Account.name::reload`)
        // We need to identify all (B) which are dominated by (A) and *not* dominated by a corresponding (C)

        // BBs terminated by a CPI
        let mut cpi_calls: HashMap<BasicBlock, Span> = HashMap::new();
        // Map of account fields to BBs accessing them
        // FIXME: Use a proper Place. Currently we assume there is exactly one account of each kind
        let mut account_accesses: HashMap<String, HashMap<BasicBlock, Span>> = HashMap::new();
        // Map of account fields to BBs reloading them
        let mut account_reloads: HashMap<String, HashSet<BasicBlock>> = HashMap::new();
        // Map of CPI context account types
        let mut cpi_accounts: HashMap<String, BasicBlock> = HashMap::new();

        // Map of account names invoked in a CPI
        let (cpi_accounts_map, reverse_assignment_map) = build_local_relationship_maps(mir);
        let transitive_assignment_reverse_map =
            build_transitive_reverse_map(&reverse_assignment_map);

        for (bb, bbdata) in mir.basic_blocks.iter_enumerated() {
            // Locate blocks ending with a call
            if let TerminatorKind::Call {
                func: Operand::Constant(func),
                args,
                fn_span,
                ..
            } = &bbdata.terminator().kind
                && let rustc_ty::FnDef(fn_def_id, _) = func.ty().kind()
            {
                let fn_sig = cx.tcx.fn_sig(*fn_def_id).skip_binder();
                let fn_sig_unbounded = fn_sig.skip_binder();
                let return_ty = fn_sig_unbounded.output();
                // Check that it is a diag item
                if let Some(diag_item) = cx
                    .tcx
                    .diagnostic_items(fn_def_id.krate)
                    .id_to_name
                    .get(fn_def_id)
                {
                    // Check if it is Account::reload...
                    if *diag_item == account_reload_sym {
                        // Extract the receiver
                        if let Some(account) = args.get(0)
                            && let Operand::Move(account) = account.node
                            && let Some(local) = account.as_local()
                        {
                            if let Some(account_name) = check_local_and_assignment_locals(
                                cx,
                                mir,
                                &local,
                                &transitive_assignment_reverse_map,
                                &mut HashSet::new(),
                            ) {
                                account_reloads
                                    .entry(account_name)
                                    .or_insert_with(HashSet::new)
                                    .insert(bb);
                            }
                        }
                    }
                    // Or a CPI invoke function
                    else if cpi_invoke_syms.contains(diag_item) {
                        cpi_calls.insert(bb, *fn_span);
                    } else if *diag_item == deref_method_sym {
                        if let Some(account) = args.get(0)
                            && let Operand::Move(account) = account.node
                            && let Some(local) = account.as_local()
                        {
                            if let Some(account_name) = check_local_and_assignment_locals(
                                cx,
                                mir,
                                &local,
                                &transitive_assignment_reverse_map,
                                &mut HashSet::new(),
                            ) {
                                account_accesses
                                    .entry(account_name)
                                    .or_insert_with(HashMap::new)
                                    .insert(bb, *fn_span);
                            }
                        }
                    }
                } else if takes_cpi_context(cx, mir, args) {
                    cpi_calls.insert(bb, *fn_span);
                }
                // CPI context
                else if is_type_diagnostic_item(cx, return_ty, anchor_cpi_sym) {
                    if let Some(cpi_accounts_struct) = args.get(1)
                        && let Operand::Copy(place) | Operand::Move(place) =
                            &cpi_accounts_struct.node
                        && let Some(accounts_local) = place.as_local()
                    {
                        find_cpi_accounts_struct(
                            &accounts_local,
                            &reverse_assignment_map,
                            &cpi_accounts_map,
                            &mut HashSet::new(),
                        )
                        .map(|accounts| {
                            for account_local in accounts {
                                if let Some(account_name) = check_local_and_assignment_locals(
                                    cx,
                                    mir,
                                    &account_local,
                                    &transitive_assignment_reverse_map,
                                    &mut HashSet::new(),
                                ) {
                                    cpi_accounts.insert(account_name, bb);
                                }
                            }
                        });
                    }
                }
            }
        }
        let cpi_call_blocks: HashSet<_> = cpi_calls.keys().copied().collect();
        cpi_accounts
            .retain(|_ty, &mut block| reachable_blocks(&mir.basic_blocks, block, &cpi_call_blocks));

        account_accesses.retain(|ty, _| cpi_accounts.contains_key(ty));
        for (ty, accesses) in account_accesses.into_iter() {
            let access_blocks = accesses.keys().copied().collect();
            let reloads = account_reloads.remove(&ty).unwrap_or_default();
            for (access, cpi) in reachable_without_passing(
                &mir.basic_blocks,
                cpi_call_blocks.clone(),
                access_blocks,
                reloads,
            ) {
                span_lint_and_note(
                    cx,
                    MISSING_ACCOUNT_RELOAD,
                    accesses[&access],
                    "accessing an account after a CPI without calling `reload()`",
                    Some(cpi_calls[&cpi]),
                    "CPI is here",
                );
            }
        }
    }
}

fn takes_cpi_context(cx: &LateContext<'_>, mir: &MirBody<'_>, args: &[Spanned<Operand>]) -> bool {
    args.iter().any(|arg| {
        if let Operand::Copy(place) | Operand::Move(place) = &arg.node
            && let Some(local) = place.as_local()
            && let Some(decl) = mir.local_decls().get(local)
        {
            is_type_diagnostic_item(cx, decl.ty.peel_refs(), Symbol::intern("AnchorCpiContext"))
        } else {
            false
        }
    })
}

/// Finds blocks in `to` that are reachable from `from` nodes without passing through `without` nodes
/// Returns a list of `to` nodes with the `from` node they are reachable from
fn reachable_without_passing(
    graph: &BasicBlocks,
    from: HashSet<BasicBlock>,
    to: HashSet<BasicBlock>,
    without: HashSet<BasicBlock>,
) -> Vec<(BasicBlock, BasicBlock)> {
    let mut queue = VecDeque::new();
    // Map of nodes to the `from` block they are reachable from
    let mut origin = HashMap::new();
    let mut visited = HashSet::new();

    for &f in &from {
        origin.insert(f, f);
        visited.insert(f);
        queue.push_back(f);
    }

    while let Some(u) = queue.pop_front() {
        if without.contains(&u) {
            continue;
        }
        for succ in graph[u]
            .terminator
            .as_ref()
            .map(|t| t.successors().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            if without.contains(&succ) || visited.contains(&succ) {
                continue;
            }
            origin.insert(succ, origin[&u]);
            visited.insert(succ);
            queue.push_back(succ);
        }
    }

    to.into_iter()
        .filter_map(|bb| origin.get(&bb).map(|o| (bb, *o)))
        .collect()
}

pub fn build_local_relationship_maps<'tcx>(
    mir: &MirBody<'tcx>,
) -> (HashMap<Local, Vec<Local>>, HashMap<Local, Vec<Local>>) {
    let mut cpi_account_local_map: HashMap<Local, Vec<Local>> = HashMap::new();
    let mut reverse_assignment_map: HashMap<Local, Vec<Local>> = HashMap::new();

    for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
        for statement in &bbdata.statements {
            if let StatementKind::Assign(box (dest_place, rvalue)) = &statement.kind {
                if let Some(dest_local) = dest_place.as_local() {
                    if let Rvalue::Aggregate(_, field_operands) = rvalue {
                        for operand in field_operands {
                            if let Operand::Copy(field_place) | Operand::Move(field_place) = operand
                            {
                                if let Some(field_local) = field_place.as_local() {
                                    cpi_account_local_map
                                        .entry(dest_local)
                                        .or_insert_with(Vec::new)
                                        .push(field_local);
                                }
                            }
                        }
                    }

                    let mut record_mapping = |src_place: &Place<'tcx>| {
                        let src_local = src_place.local;
                        reverse_assignment_map
                            .entry(src_local)
                            .or_insert_with(Vec::new)
                            .push(dest_local);
                    };

                    match rvalue {
                        Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) => record_mapping(src),
                        Rvalue::Ref(_, _, src) => record_mapping(src),
                        Rvalue::Cast(_, op, _) => {
                            if let Operand::Copy(src) | Operand::Move(src) = op {
                                record_mapping(src);
                            }
                        }
                        Rvalue::Aggregate(_, operands) => {
                            for operand in operands {
                                if let Operand::Copy(src) | Operand::Move(src) = operand {
                                    record_mapping(src);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    (cpi_account_local_map, reverse_assignment_map)
}

fn reachable_blocks(graph: &BasicBlocks, from: BasicBlock, to: &HashSet<BasicBlock>) -> bool {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    visited.insert(from);
    queue.push_back(from);

    while let Some(u) = queue.pop_front() {
        if to.contains(&u) {
            return true;
        }
        for succ in graph[u]
            .terminator
            .as_ref()
            .map(|t| t.successors().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            if visited.contains(&succ) {
                continue;
            }
            visited.insert(succ);
            queue.push_back(succ);
        }
    }
    false
}

fn extract_context_account(line: &str) -> Option<String> {
    // Remove comments from the line before processing
    let snippet = line
        .split("//")
        .next()
        .unwrap_or(line)
        .split("/*")
        .next()
        .unwrap_or(line)
        .trim();
    if let Some(start) = snippet.find(".accounts.") {
        let prefix_start = snippet[..start]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let prefix = &snippet[prefix_start..start]; // e.g., "ctx"

        let rest = &snippet[start + ".accounts.".len()..];

        let account_name_end = rest
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        let account = &rest[..account_name_end]; // e.g., "user"

        Some(format!("{}.accounts.{}", prefix, account))
    } else {
        None
    }
}

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

fn check_local_and_assignment_locals<'tcx>(
    cx: &LateContext<'tcx>,
    mir: &MirBody<'_>,
    account_local: &Local,
    transitive_assignment_reverse_map: &HashMap<Local, Vec<Local>>,
    visited: &mut HashSet<Local>,
) -> Option<String> {
    let local_decl = &mir.local_decls[*account_local];
    let span = local_decl.source_info.span;
    if let Ok(snippet) = cx.sess().source_map().span_to_snippet(span) {
        if let Some(account_name) = extract_context_account(&snippet) {
            return Some(account_name);
        } else {
            if let Ok(file_span) = cx.sess().source_map().span_to_lines(span) {
                let file = &file_span.file;
                let start_line_idx = file_span.lines[0].line_index;
                if let Some(src) = file.src.as_ref() {
                    let lines: Vec<&str> = src.lines().collect();
                    if let Some(account_name) = extract_context_account(&lines[start_line_idx]) {
                        return Some(account_name);
                    }
                }
            }
        }
    }
    if visited.contains(account_local) {
        return None;
    }
    visited.insert(*account_local);

    for (lhs, rhs) in transitive_assignment_reverse_map {
        if rhs.contains(account_local) {
            // recursively check the lhs
            if let Some(result) = check_local_and_assignment_locals(
                cx,
                mir,
                lhs,
                transitive_assignment_reverse_map,
                visited,
            ) {
                return Some(result);
            }
        }
    }

    None
}

fn find_cpi_accounts_struct(
    account_stuct_local: &Local,
    reverse_assignment_map: &HashMap<Local, Vec<Local>>,
    cpi_accounts_map: &HashMap<Local, Vec<Local>>,
    visited: &mut HashSet<Local>,
) -> Option<Vec<Local>> {
    if let Some(accounts) = cpi_accounts_map.get(account_stuct_local) {
        return Some(accounts.clone());
    }
    if visited.contains(account_stuct_local) {
        return None;
    }
    visited.insert(*account_stuct_local);
    for (lhs, rhs) in reverse_assignment_map {
        if rhs.contains(account_stuct_local) {
            // recursively check the lhs
            if let Some(accounts) =
                find_cpi_accounts_struct(lhs, reverse_assignment_map, cpi_accounts_map, visited)
            {
                return Some(accounts);
            }
        }
    }
    None
}

#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use anchor_lints_utils::mir_analyzer::MirAnalyzer;

use clippy_utils::{diagnostics::span_lint_and_note, fn_has_unsatisfiable_preds};
use rustc_hir::{Body as HirBody, FnDecl, def_id::LocalDefId, intravisit::FnKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_span::Span;
use std::cell::RefCell;
use std::collections::HashSet;

mod utils;
use utils::{
    account_extraction::extract_init_accounts_and_inner_types,
    field_analysis::{extract_inner_struct_fields, should_ignore_field},
    mir_analysis::collect_account_field_assignments,
};

// Track (account_name, def_id) pairs we've already warned about to avoid duplicates
// Track spans of trait method calls to avoid linting accounts initialized via trait methods
thread_local! {
    static WARNED_ACCOUNTS: RefCell<HashSet<Span>> = RefCell::new(HashSet::new());
    static TRAIT_METHOD_ACCOUNTS: RefCell<HashSet<Span>> = RefCell::new(HashSet::new());
}

dylint_linting::declare_late_lint! {
    /// ### What it does
    /// Detects initialization handlers for `#[account(init, ...)]` accounts that
    /// do **not** assign all fields of the account struct.
    ///
    /// ### Why is this bad?
    /// Leaving fields at their default zeroed value can cause subtle logic bugs,
    /// and in some cases security issues (e.g., forgotten authority or limits).
    ///
    /// ### Bad
    /// ```rust
    /// #[account]
    /// pub struct Collection {
    ///     pub authority: Pubkey,
    ///     pub lifetime_tokens_collected: u64,
    ///     pub max_collectable_tokens: u64,
    /// }
    ///
    /// pub fn init_collection(ctx: Context<InitCollection>, max_collectable_tokens: u64) -> Result<()> {
    ///     let collection = &mut ctx.accounts.collection;
    ///     collection.authority = ctx.accounts.authority.key();
    ///     collection.lifetime_tokens_collected = 0;
    ///     // forgets to set `max_collectable_tokens`
    ///     Ok(())
    /// }
    /// ```
    ///
    /// ### Good
    /// ```rust
    /// pub fn init_collection(ctx: Context<InitCollection>, max_collectable_tokens: u64) -> Result<()> {
    ///     let collection = &mut ctx.accounts.collection;
    ///     collection.authority = ctx.accounts.authority.key();
    ///     collection.lifetime_tokens_collected = 0;
    ///     collection.max_collectable_tokens = max_collectable_tokens;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// ### Known Limitations
    /// - **Trait method initialization**: If an account is `AccountLoader<'info, T>` and is
    ///   initialized via a trait method (e.g., `account.initialize(...)`), the lint will not
    ///   flag uninitialized fields. This is because trait method implementations are difficult
    ///   to analyze statically without knowing the concrete receiver type at compile time.
    ///   The lint treats such cases as safe to avoid false positives, but fields may still
    ///   be uninitialized if the trait method doesn't set them all.
    ///
    ///   Example:
    ///   ```rust
    ///   let mut account = account_loader.load_init()?;
    ///   account.initialize(...); // Trait method - fields set here won't be detected
    ///   ```
    pub MISSING_ACCOUNT_FIELD_INIT,
    Warn,
    "account initialized with some fields left unset"
}

impl<'tcx> LateLintPass<'tcx> for MissingAccountFieldInit {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        _kind: FnKind<'tcx>,
        _decl: &'tcx FnDecl<'tcx>,
        body: &'tcx HirBody<'tcx>,
        fn_span: Span,
        def_id: LocalDefId,
    ) {
        // Skip macro expansions
        if fn_span.from_expansion() {
            return;
        }

        // Skip functions with unsatisfiable predicates
        if fn_has_unsatisfiable_preds(cx, def_id.to_def_id()) {
            return;
        }

        analyze_missing_account_field_init(cx, body, def_id, fn_span);
    }
}

fn analyze_missing_account_field_init<'tcx>(
    cx: &LateContext<'tcx>,
    body: &'tcx HirBody<'tcx>,
    def_id: LocalDefId,
    fn_span: Span,
) {
    let mir_analyzer = MirAnalyzer::new(cx, body, def_id);
    let Some(anchor_context) = mir_analyzer.anchor_context_info.as_ref() else {
        return;
    };

    let init_accounts = extract_init_accounts_and_inner_types(cx, anchor_context);
    if init_accounts.is_empty() {
        return;
    }

    let mut account_fields = std::collections::HashMap::new();
    for (account_name, info) in &init_accounts {
        if let Some(fields) = extract_inner_struct_fields(cx, info.inner_ty) {
            account_fields.insert(account_name.clone(), fields);
        }
    }
    let field_assignments =
        collect_account_field_assignments(cx, &mir_analyzer, def_id, &init_accounts, fn_span);

    for (account_name, info) in init_accounts {
        // Check if we've already warned about this account in this function
        let already_warned = WARNED_ACCOUNTS.with(|warned| warned.borrow().contains(&info.span));
        if already_warned {
            continue;
        }

        // Skip is account is an AccountLoader and has trait method calls
        if info.is_account_loader {
            let is_trait_method = TRAIT_METHOD_ACCOUNTS
                .with(|trait_method_accounts| trait_method_accounts.borrow().contains(&fn_span));
            if is_trait_method {
                continue;
            }
        }

        if let Some(fields) = account_fields.get(&account_name) {
            let assigned = field_assignments
                .get(&account_name)
                .cloned()
                .unwrap_or_default();

            let mut missing = Vec::new();
            for f in fields {
                if should_ignore_field(cx, f) {
                    continue;
                }
                if !assigned.contains(&f.name) {
                    missing.push(f.name.clone());
                }
            }

            if !missing.is_empty() {
                // Mark this (account_name, def_id) as warned
                WARNED_ACCOUNTS.with(|warned| {
                    warned.borrow_mut().insert(info.span);
                });
                span_lint_and_note(
                    cx,
                    MISSING_ACCOUNT_FIELD_INIT,
                    info.span,
                    format!(
                        "account `{}` is initialized but the following fields are never assigned: {}",
                        account_name,
                        missing.join(", "),
                    ),
                    Some(fn_span),
                    "In this function",
                );
            }
        }
    }
}

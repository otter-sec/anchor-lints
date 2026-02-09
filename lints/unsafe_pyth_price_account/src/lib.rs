#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use anchor_lints_utils::{
    mir_analyzer::{AnchorContextInfo, MirAnalyzer},
    utils::pda_detection::is_pda_account,
};

use anchor_lints_utils::utils::should_skip_function;
use clippy_utils::diagnostics::span_lint;
use rustc_hir::{Body as HirBody, FnDecl, def_id::LocalDefId, intravisit::FnKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::{
    mir::{Operand, TerminatorKind},
    ty::{Ty, TyKind},
};
use rustc_span::{DUMMY_SP, Span, sym};

use std::collections::HashMap;

mod checks;
use checks::*;

#[derive(Default)]
struct PriceAccountUsage {
    account_span: rustc_span::Span,
    has_get_price_call: bool,
    has_field_access: bool,
    has_pubkey_check: bool,
    has_monotonic_publish_time: bool,
    get_price_span: Option<rustc_span::Span>,
}

dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Detects unsafe usage of Pyth PriceUpdateV2 accounts where a program relies on `feed_id` and
    /// `max_age` validation but does not enforce canonical price sources or monotonic publish times.
    ///
    /// ### Why is this bad?
    /// Using non-canonical Pyth price feeds or not enforcing monotonic publish times can allow
    /// attackers to provide stale or manipulated price data, leading to incorrect pricing decisions
    /// and potential fund loss.
    ///
    /// ### Bad
    /// ```rust
    /// pub price_account: Account<'info, PriceUpdateV2>,
    ///
    /// let feed_id = get_feed_id_from_hex(FEED_ID)?;
    /// let price = price_account.get_price_no_older_than(&clock, max_age, &feed_id)?;
    /// // Missing: canonical feed address check or monotonic publish_time enforcement
    /// ```
    ///
    /// ### Good
    /// ```rust
    /// // Option 1: Check against canonical feed address
    /// require_keys_eq!(price_account.key(), CANONICAL_FEED_ADDRESS);
    /// let feed_id = get_feed_id_from_hex(FEED_ID)?;
    /// let price = price_account.get_price_no_older_than(&clock, max_age, &feed_id)?;
    ///
    /// // Option 2: Enforce monotonic publish times (both comparison AND storage required)
    /// let feed_id = get_feed_id_from_hex(FEED_ID)?;
    /// let price = price_account.get_price_no_older_than(&clock, max_age, &feed_id)?;
    /// require!(
    ///     price_account.price_message.publish_time > state.last_publish_time,
    ///     ErrorCode::StalePrice
    /// );
    /// state.last_publish_time = price_account.price_message.publish_time;
    /// ```
    pub UNSAFE_PYTH_PRICE_ACCOUNT,
    Warn,
    "Pyth PriceUpdateV2 account used without canonical source validation or monotonic publish time enforcement",
    UnsafePythPriceAccount
}

#[derive(Default)]
pub struct UnsafePythPriceAccount;

impl<'tcx> LateLintPass<'tcx> for UnsafePythPriceAccount {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        _kind: FnKind<'tcx>,
        _: &FnDecl<'tcx>,
        body: &HirBody<'tcx>,
        main_fn_span: Span,
        def_id: LocalDefId,
    ) {
        // Skip macro expansions, unsatisfiable predicates, and test files
        if should_skip_function(cx, main_fn_span, def_id) {
            return;
        }

        let mut mir_analyzer = MirAnalyzer::new(cx, body, def_id);

        // Update anchor context info with accounts
        anchor_lints_utils::utils::ensure_anchor_context_initialized(&mut mir_analyzer, body);

        // Analyze functions that take Anchor context
        let Some(anchor_context_info) = mir_analyzer.anchor_context_info.as_ref() else {
            return;
        };

        analyze_unsafe_pyth_price_accounts(cx, &mir_analyzer, anchor_context_info);
    }
}

fn analyze_unsafe_pyth_price_accounts<'cx, 'tcx>(
    cx: &'cx LateContext<'tcx>,
    mir_analyzer: &MirAnalyzer<'cx, 'tcx>,
    anchor_context_info: &AnchorContextInfo<'tcx>,
) {
    let accounts_struct_ty = &anchor_context_info.anchor_context_account_type;

    if let TyKind::Adt(adt_def, generics) = accounts_struct_ty.kind() {
        if !adt_def.is_struct() && !adt_def.is_union() {
            return;
        }

        let variant = adt_def.non_enum_variant();
        let mir = mir_analyzer.mir;

        let mut price_accounts: Vec<(String, Span, Ty<'tcx>)> = Vec::new();
        let mut price_account_usage: HashMap<String, PriceAccountUsage> = HashMap::new();

        for field in &variant.fields {
            let account_name = field.ident(cx.tcx).to_string();
            let account_ty = field.ty(cx.tcx, generics);

            // Check if this is a PriceUpdateV2 account
            if is_account_price_update_v2(cx, account_ty) {
                // Skip if it's a PDA
                if is_pda_account(cx, field).is_some() {
                    continue;
                }
                price_accounts.push((account_name, cx.tcx.def_span(field.did), account_ty));
            }
        }

        if price_accounts.is_empty() {
            return;
        }

        for (_bb, bbdata) in mir.basic_blocks.iter_enumerated() {
            if let TerminatorKind::Call {
                func: Operand::Constant(func),
                args,
                fn_span,
                ..
            } = &bbdata.terminator().kind
                && let TyKind::FnDef(fn_def_id, _) = func.ty().kind()
            {
                // Check for get_price_no_older_than calls
                if is_get_price_no_older_than(cx, *fn_def_id) {
                    if let Some(price_account_name) =
                        extract_price_account_from_args(mir_analyzer, args)
                    {
                        // Only track if this account is in the price_accounts list
                        if price_accounts.iter().any(|(name, _, _)| {
                            price_account_name == *name
                                || price_account_name.ends_with(&format!(".{}", name))
                        }) {
                            let (account_span, short_name) =
                                find_account_info(&price_account_name, &price_accounts);
                            let usage =
                                price_account_usage.entry(short_name).or_insert_with(|| {
                                    PriceAccountUsage {
                                        account_span,
                                        has_get_price_call: false,
                                        has_field_access: false,
                                        has_pubkey_check: false,
                                        has_monotonic_publish_time: false,
                                        get_price_span: None,
                                    }
                                });
                            usage.has_get_price_call = true;
                            usage.get_price_span = Some(*fn_span);
                        }
                    }
                }
                // Check for direct field access
                else if cx.tcx.is_diagnostic_item(sym::deref_method, *fn_def_id)
                    && let Some(price_account_name) =
                        extract_price_account_from_args(mir_analyzer, args)
                {
                    // Only track if this account is in the price_accounts list
                    if price_accounts.iter().any(|(name, _, _)| {
                        price_account_name == *name
                            || price_account_name.ends_with(&format!(".{}", name))
                    }) {
                        let (account_span, short_name) =
                            find_account_info(&price_account_name, &price_accounts);
                        let usage = price_account_usage.entry(short_name).or_insert_with(|| {
                            PriceAccountUsage {
                                account_span,
                                has_get_price_call: false,
                                has_field_access: false,
                                has_pubkey_check: false,
                                has_monotonic_publish_time: false,
                                get_price_span: None,
                            }
                        });
                        usage.has_field_access = true;
                    }
                }
            }
        }

        // Check for pubkey comparisons and publish_time monotonicity
        for (account_name, usage) in &mut price_account_usage {
            // Check if account key is compared
            if has_pubkey_constant_check(cx, mir_analyzer, account_name) {
                usage.has_pubkey_check = true;
            }

            // Check if publish_time is stored and compared for monotonicity
            if has_monotonic_publish_time_enforcement(
                cx,
                mir_analyzer,
                account_name,
                anchor_context_info,
            ) {
                usage.has_monotonic_publish_time = true;
            }
        }

        for (account_name, usage) in price_account_usage {
            if (usage.has_get_price_call || usage.has_field_access)
                && !usage.has_pubkey_check
                && !usage.has_monotonic_publish_time
            {
                let span = usage.get_price_span.unwrap_or(usage.account_span);
                span_lint(
                    cx,
                    UNSAFE_PYTH_PRICE_ACCOUNT,
                    span,
                    format!(
                        "Pyth PriceUpdateV2 account `{}` is used without canonical source validation or monotonic publish time enforcement. Consider comparing the account's pubkey against a known canonical feed address, or storing and enforcing monotonicity on publish_time.",
                        account_name
                    ),
                );
            }
        }
    }
}

/// Find account span and short name from price_accounts list
fn find_account_info<'tcx>(
    price_account_name: &str,
    price_accounts: &[(String, Span, Ty<'tcx>)],
) -> (Span, String) {
    price_accounts
        .iter()
        .find(|(name, _, _)| {
            price_account_name == *name || price_account_name.ends_with(&format!(".{}", name))
        })
        .map(|(name, span, _)| (*span, name.clone()))
        .unwrap_or((DUMMY_SP, price_account_name.to_string()))
}

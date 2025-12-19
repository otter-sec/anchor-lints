# `missing_account_field_init`

### What it does
Detects initialization handlers for `#[account(init, ...)]` accounts that do not assign all fields of the account struct.

### Why is this bad?
Leaving fields at their default zeroed value can cause subtle logic bugs and security issues, such as forgotten authority or limits that allow unauthorized access or incorrect behavior.

### Known Limitations
If an account is `AccountLoader<'info, T>` and is initialized via a trait method (e.g., `account.initialize(...)`), the lint will not flag uninitialized fields. This is because trait method implementations are difficult to analyze statically without knowing the concrete receiver type at compile time. The lint treats such cases as safe to avoid false positives, but fields may still be uninitialized if the trait method doesn't set them all.

# anchor-lints

A collection of security-focused lints for Anchor/Solana programs using [dylint](https://github.com/trailofbits/dylint).

## Installation

```bash
cargo install cargo-dylint dylint-link
```

## Available Lints

### `missing_account_reload`

Detects account access after CPI without calling `reload()`. After a CPI, deserialized accounts don't update automatically, which can lead to stale data being used.

### `arbitrary_cpi_call`

Detects CPI calls with user-controlled program IDs without validation. This can allow attackers to trigger arbitrary programs, leading to security vulnerabilities.

### `duplicate_mutable_accounts`

Detects duplicate mutable account usage in functions where the same account is passed into multiple mutable parameters, which can cause unexpected aliasing and state corruption.

### `cpi_no_result`

Detects Cross-Program Invocation (CPI) calls where the result is silently suppressed using methods like `unwrap_or_default()` or `unwrap_or(())`. Silent suppression methods hide failures, allowing the program to continue execution even when critical operations failed, leading to silent failures, security vulnerabilities, potential fund loss, and state corruption.

### `pda_signer_account_overlap`

Detects when user-controlled accounts (`UncheckedAccount` or `Option<UncheckedAccount>`) are passed to CPIs that use PDAs as signers. This could lead to PDA initialization vulnerabilities if the callee expects the account to be uninitialized. An attacker could pass the PDA signer itself as the account, causing the PDA to be initialized and losing its lamports.

### `missing_signer_validation`

Detects when a CPI requires a signer (e.g., `authority`, `owner`, `current_authority`, `from`) but the account passed to that signer position is not validated as a signer. The account must be either declared as a signer (`Signer<'info>` or `#[account(signer)]`) or invoked as a PDA signer using `CpiContext::new_with_signer`. Missing signer validation allows attackers to perform unauthorized token transfers, minting, burning, authority changes, or system transfers.

### `missing_owner_check`

Detects when an `UncheckedAccount` or `AccountInfo` has its data accessed without a statically detectable owner validation. Missing owner validation allows attackers to pass accounts owned by unexpected programs, leading to reading or modifying data from wrong accounts, security vulnerabilities, and state corruption.

### `missing_account_field_init`

Detects initialization handlers for `#[account(init, ...)]` accounts that do not assign all fields of the account struct. Leaving fields at their default zeroed value can cause subtle logic bugs and security issues, such as forgotten authority or limits that allow unauthorized access or incorrect behavior.

### `ata_should_use_init_if_needed`

Detects Associated Token Accounts (ATAs) that use `init` constraint instead of `init_if_needed`. Using `init` on an ATA will fail if the account already exists. `init_if_needed` will only initialize the account if it doesn't exist, making the instruction idempotent and preventing transaction failures when the ATA already exists.

### `direct_lamport_cpi_dos`

Detects when accounts with direct lamport mutations (via `lamports.borrow_mut()`) are not included in subsequent CPI calls. The Solana runtime performs balance checks on CPI calls, and if an account's lamports were directly mutated but the account is not included in the CPI, the balance check will fail, causing a DoS.

### `overconstrained_seed_account`

Detects when a seed account used in PDA derivation is overconstrained as `SystemAccount` in non-initialization instructions. If a seed account's ownership changes after pool creation (e.g., becomes a token account or mint), future instructions will fail forever because `SystemAccount` enforces `owner == system_program`. This can permanently lock funds in the protocol.

## Usage

Run all lints on your Anchor project:

```bash
cargo dylint --path /path/to/anchor-lints/lints --pattern "*"
```

Run a specific lint:

```bash
cargo dylint --path /path/to/anchor-lints/lints --pattern "missing_account_reload"
```

## Testing

We use integration tests instead of dylint UI tests because anchor programs require external Cargo dependencies (like anchor-lang), which UI tests cannot resolve. Our tests run cargo dylint on a small standalone Anchor program, giving us a realistic environment that matches how these lints are actually used.

Run all lint tests:

```bash
cargo test
```

Run a specific lint test:

```bash
cargo test missing_account_reload_tests
cargo test duplicate_mutable_accounts_tests
cargo test arbitrary_cpi_call_tests
cargo test cpi_no_result_tests
cargo test pda_signer_account_overlap_tests
cargo test missing_signer_validation_tests
cargo test missing_owner_check_tests
cargo test missing_account_field_init_tests
cargo test ata_should_use_init_if_needed_tests
cargo test direct_lamport_cpi_dos_tests
cargo test overconstrained_seed_account_tests
```

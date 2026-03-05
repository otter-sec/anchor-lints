# anchor-lints

A collection of security-focused lints for Anchor/Solana programs using [dylint](https://github.com/trailofbits/dylint).

## Note

This repository is a work-in-progress and the lints are currently being actively developed and updated.

## Installation

```bash
cargo install cargo-dylint dylint-link
```

## Available Lints

| Lint |
| --- |
| [`missing_account_reload`](lints/missing_account_reload) |
| [`arbitrary_cpi_call`](lints/arbitrary_cpi_call) |
| [`duplicate_mutable_accounts`](lints/duplicate_mutable_accounts) |
| [`cpi_no_result`](lints/cpi_no_result) |
| [`pda_signer_account_overlap`](lints/pda_signer_account_overlap) |
| [`missing_signer_validation`](lints/missing_signer_validation) |
| [`missing_owner_check`](lints/missing_owner_check) |
| [`missing_account_field_init`](lints/missing_account_field_init) |
| [`ata_should_use_init_if_needed`](lints/ata_should_use_init_if_needed) |
| [`direct_lamport_cpi_dos`](lints/direct_lamport_cpi_dos) |
| [`overconstrained_seed_account`](lints/overconstrained_seed_account) |
| [`unsafe_pyth_price_account`](lints/unsafe_pyth_price_account) |

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
cargo test unsafe_pyth_price_account_tests
```

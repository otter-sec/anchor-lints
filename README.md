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

Run the `missing_account_reload` lint tests:

```bash
cargo test missing_account_reload_tests
```

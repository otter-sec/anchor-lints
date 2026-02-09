# Contributing to Tests

This document explains how to add and maintain tests for the Anchor lints.

## Testing Approach

We test lints using a separate Anchor program instead of dylint UI tests. 
UI tests compile files directly with rustc, which means Cargo dependencies (like anchor-lang) are not resolved. Since Anchor programs require real dependencies, UI tests cannot build them.

Integration tests run `cargo dylint` on a real Anchor program, matching how users will actually run the lints.

## Test Structure
The test Anchor program lives in: `lints/missing_account_reload/tests/test_program/`

A test runner (tests/lint_tests.rs) invokes `cargo dylint` on this program and checks whether lint diagnostics match the markers inside the program’s source files.


### Marker Comments

- **`[cpi_call]`**: Marks the line where a CPI occurs. The lint should identify this as the source of potential stale data.

- **`[unsafe_account_accessed]`**: Marks a line where an account is accessed after a CPI without calling `reload()`. The lint **should** trigger a warning on this line.

- **`[safe_account_accessed]`**: Marks a line where an account is accessed after a CPI, but `reload()` has been called. The lint **should NOT** trigger a warning on this line.

### Example Usage

```rust
pub fn example_function(ctx: Context<Example>, amount: u64) -> Result<()> {
    let cpi_context = CpiContext::new(
        program_id,
        Transfer { /* ... */ },
    );

    transfer(cpi_context, amount)?; // [cpi_call]
    
    // This should trigger the lint - no reload() called
    let _data = ctx.accounts.my_account.data; // [unsafe_account_accessed]
    
    Ok(())
}
```

```rust
pub fn safe_example(ctx: Context<Example>, amount: u64) -> Result<()> {
    let cpi_context = CpiContext::new(
        program_id,
        Transfer { /* ... */ },
    );

    transfer(cpi_context, amount)?;
    
    // Reload before access - should NOT trigger lint
    ctx.accounts.my_account.reload()?;
    let _data = ctx.accounts.my_account.data; // [safe_account_accessed]
    
    Ok(())
}
```

## Adding New Test Cases

1. **Add test function**: Create a new function in `lints/missing_account_reload/tests/test_program/src/lib.rs` that demonstrates a pattern you want to test.

2. **Add marker comments**: Use the appropriate markers (`[cpi_call]`, `[unsafe_account_accessed]`, `[safe_account_accessed]`) to mark expected behavior.

3. **Run tests**: Execute `cargo test missing_account_reload` to verify your test case works correctly.

## How Tests Are Validated

The test runner (`tests/lint_tests.rs`) performs the following steps:

1. **Parse markers**: Scans all Rust files in the test program and extracts marker comments with their line numbers.

2. **Run lint**: Executes `cargo dylint` on the test program with the appropriate lint pattern.

3. **Parse output**: Extracts warning messages and line numbers from the lint output.

4. **Validate**:
   - Lines marked with `[unsafe_account_accessed]` must have corresponding warnings in the lint output.
   - Lines marked with `[safe_account_accessed]` must **not** have warnings in the lint output.
   - Lines marked with `[cpi_call]` are used to identify the CPI location in warning messages.

5. **Report**: If any mismatch is found, the test fails with detailed information about missing or unexpected warnings.

## Running Tests

To run only the `missing_account_reload` tests:
```bash
cargo test missing_account_reload
```

# `cpi_no_result`

### What it does
Flags CPI calls when **discard the result** with one of these methods:

- `unwrap_or_default()`
- `unwrap_or(())` or `unwrap_or(some_value)`
- `unwrap_or_else(|_| ...)`

### Why it matters
Discarding the CPI result makes it unclear how failures are handled. Even though many CPI failures abort the transaction, hiding the result makes the code harder to understand and debug. Using ? or explicit error handling makes it clear when a CPI can fail and ensures failures are handled in a consistent and readable way.

### Example

**Flagged:**
```rust
system_program::transfer(cpi_ctx, amount).unwrap_or_default();
system_program::transfer(cpi_ctx, amount).unwrap_or(());
system_program::transfer(cpi_ctx, amount).unwrap_or_else(|_| ());
```

**OK():**
```rust
system_program::transfer(cpi_ctx, amount)?;
system_program::transfer(cpi_ctx, amount).unwrap();
system_program::transfer(cpi_ctx, amount).expect("Transfer failed");
```


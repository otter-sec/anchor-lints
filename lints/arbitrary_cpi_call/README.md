# `arbitrary_cpi_call`

### What it does
Identifies CPI calls made using user-controlled program IDs (from accounts or parameters) without validations before the CPI call.

### Why is this bad?
Unvalidated program IDs in CPI calls let users to trigger arbitrary programs, leading to potential security breaches or fund loss.

### Limitation
To avoid heavy analysis, we skip nested function analysis when:

- **Cmps/switches threshold:** The number of program_id comparisons or if/else switches in the current function exceeds `MAX_CMPS_SWITCHES_RECURSION_THRESHOLD`.
- **If/else nesting level:** The current basic block is nested deeper than `MAX_IF_ELSE_NESTING_LEVEL` depth (number of dominating `SwitchInt` blocks). 

When any one of the condition triggers, we still run CPI checks for the current function (e.g. we still report arbitrary CPI in that function). We only skip propagating validation from nested functions, so very large or deeply nested code may not get full inter-procedural analysis for now.

### Example (worst case)

```rust
// BAD: program_id is user-controlled and never validated
pub fn invoke_unchecked_program(ctx: Context<DirectInvokeTransfer>) -> Result<()> {
    use anchor_lang::solana_program::instruction::Instruction;
    use anchor_lang::solana_program::program::invoke;
    let instruction = Instruction {
        program_id: ctx.accounts.unchecked_program.key(),  // user can pass any program
        accounts: vec![],
        data: vec![],
    };
    let account_infos = vec![ctx.accounts.unchecked_program.to_account_info()];
    invoke(&instruction, &account_infos)?;  // CPI
    Ok(())
}

// GOOD: program_id validated against a constant before CPI
pub fn invoke_validated_program(ctx: Context<DirectInvokeTransfer>) -> Result<()> {
    use anchor_lang::solana_program::instruction::Instruction;
    use anchor_lang::solana_program::program::invoke;
    const ALLOWED_PROGRAM_ID: Pubkey = Pubkey::new_from_array([42u8; 32]);
    require_keys_eq!(
        ctx.accounts.unchecked_program.key(),
        ALLOWED_PROGRAM_ID,
        CustomError::InvalidProgram
    );
    let instruction = Instruction {
        program_id: ctx.accounts.unchecked_program.key(),
        accounts: vec![],
        data: vec![],
    };
    let account_infos = vec![ctx.accounts.unchecked_program.to_account_info()];
    invoke(&instruction, &account_infos)?;  // CPI
    Ok(())
}
```

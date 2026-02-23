# `arbitraty_cpi_call`

### What it does
Identifies CPI calls made using user-controlled program IDs without validations.

### Why is this bad?
Unvalidated program IDs in CPI calls let users to trigger arbitrary programs, leading to potential security breaches or fund loss.

### Limitation
To avoid heavy analysis, we skip nested function analysis when:

- **Cmps/switches threshold:** The number of program_id comparisons or if/else switches in the current function exceeds `MAX_CMPS_SWITCHES_RECURSION_THRESHOLD`.
- **If/else nesting level:** The current basic block is nested deeper than `MAX_IF_ELSE_NESTING_LEVEL` depth (number of dominating `SwitchInt` blocks). 

When any one of the condition triggers, we still run CPI checks for the current function (e.g. we still report arbitrary CPI in that function). We only skip propagating validation from nested functions, so very large or deeply nested code may not get full inter-procedural analysis for now.
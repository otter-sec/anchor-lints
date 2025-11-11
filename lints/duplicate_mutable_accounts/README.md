# `duplicate_mutable_accounts`

### What it does
Detects duplicate mutable account usage in Anchor functions, where the same account type is passed into multiple mutable parameters without constraint checks.

### Why is this bad?
Duplicate mutable accounts can lead to unexpected aliasing of mutable data, logical errors, and vulnerabilities like account state corruption in Solana smart contracts.
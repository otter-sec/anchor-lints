# `overconstrained_seed_account`

### What it does
Detects when a seed account used in PDA derivation is overconstrained as `SystemAccount` in non-initialization instructions.

### Why is this bad?
If a seed account's ownership changes after pool creation (e.g., becomes a token account or mint), future instructions will fail forever because `SystemAccount` enforces `owner == system_program`. This can permanently lock funds in the protocol.


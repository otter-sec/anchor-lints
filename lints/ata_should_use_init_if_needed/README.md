# `ata_should_use_init_if_needed`

### What it does
Detects Associated Token Accounts (ATAs) that use `init` constraint instead of `init_if_needed`.

### Why is this bad?
Using `init` on an ATA will fail if the account already exists. `init_if_needed` will only initialize the account if it doesn't exist, making the instruction idempotent and preventing transaction failures when the ATA already exists.


# quasar-field-test

A hands-on field test of the [Quasar](https://github.com/blueshift-gg/quasar) Solana framework.

This repo contains a counter program written from scratch using Quasar, along with a detailed write-up of everything we found -- the good, the bad, and the workarounds.

See **[FIELD_TEST.md](FIELD_TEST.md)** for the full report.

## Quick Start

```bash
cargo install quasar-cli
git clone https://github.com/AngryPacifist/quasar-field-test.git
cd quasar-field-test
quasar build
quasar test
```

## Key Findings

- Quasar vault deposit: **1,576 CU** vs Pinocchio vault deposit: **2,833 CU** (44% cheaper)
- Quasar vault withdraw: **410 CU** vs Pinocchio vault withdraw: **1,635 CU** (75% cheaper)
- 692/692 repo tests pass
- Found a bug in quasar-svm: CPI-created accounts not committed across `process_instruction` calls
- Windows not supported (requires WSL)
- Several undocumented patterns discovered (Pod arithmetic, set_inner, ctx.bumps)

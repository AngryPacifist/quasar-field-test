# Quasar Field Test

**Date:** March 23, 2026
**Framework:** Quasar v0.0.0 (pre-release, unaudited)
**Repository:** https://github.com/blueshift-gg/quasar
**Environment:** WSL Ubuntu, Rust 1.94.0, cargo-build-sbf 3.0.13

---

## What This Is

A hands-on, end-to-end field test of the Quasar Solana framework. We cloned the repo, read every file, built every example, ran every test, benchmarked the compute unit costs, scaffolded a new project from scratch, wrote a custom program, and documented everything we found along the way.

The goal was to answer one question: is Quasar real, or is it vapor?

---

## What We Tested

### Building the Existing Codebase

The Quasar repository is a Cargo workspace with 13 top-level directories and roughly 23 crates. We built all SBF binaries using `cargo build-sbf` via the provided Makefile. Nine programs compiled successfully: three example programs (vault, escrow, multisig), six test programs (accounts, errors, events, misc, PDA, sysvar, token CPI). We also compiled a raw Pinocchio vault from the repo's examples for comparison.

### Running the Test Suite

The repository ships 692 tests. We ran every single one.

**692 passed. 0 failed.**

The breakdown:

| Suite | Tests | Coverage |
|---|---|---|
| Integration suite | 437 | Accounts, validation, constraints, CPI, dynamic fields, PDA, events, sysvars, token CPI, remaining accounts |
| Miri (lang) | 79 | Pointer aliasing, buffer bounds, CPI data, remaining accounts, MaybeUninit, dynamic types |
| Pod types | 72 | Arithmetic, bitwise, shifts, checked/saturating ops, endianness, alignment |
| Miri (SPL) | 67 | Token/mint casting, interface accounts, CPI data init, zero-copy deref |
| Escrow | 9 | Make, take, refund, existing token accounts, CU benchmarks |
| Core unit | 7 | keys_eq optimization, is_system_program, system CPI |
| Multisig | 7 | Create, deposit, execute_transfer, set_label, threshold enforcement |
| Vault | 3 | Deposit, withdraw, program ID |
| Compile-fail | 11 | Dynamic_vec alignment, fixed after dynamic, multiple tails, zero discriminator |

### Benchmarking Compute Units

The CU numbers come from the test output logs -- actual SVM execution via mollusk-svm, not estimates.

**Quasar Vault:**
- Deposit (system transfer CPI to PDA vault): **1,576 CU**
- Withdraw (set_lamports from PDA vault): **410 CU**

**Quasar Escrow:**
- Make (init escrow PDA + init_if_needed ATAs + token transfer + event): **29,292 CU**
- Take (validate escrow + two token transfers + close vault ATA + event): **21,573 CU**
- Refund (token transfer back + close escrow + event): **8,516 CU**

**Quasar Multisig:**
- Create (init PDA, write signer array, set label): **2,013 CU**
- Execute transfer (iterate signers, count threshold, signed CPI transfer): **3,774 CU**

### CU Comparison: Quasar vs Raw Pinocchio

The repository ships a raw Pinocchio implementation of the same vault doing the same thing -- accept SOL deposits to a PDA, allow SOL withdrawals. We built and tested both under the same conditions.

| Operation | Quasar Vault | Pinocchio Vault | Quasar Savings |
|---|---|---|---|
| **Deposit** | **1,576 CU** | **2,833 CU** | **44% cheaper** |
| **Withdraw** | **410 CU** | **1,635 CU** | **75% cheaper** |

Quasar -- a framework -- beats hand-written Pinocchio -- the lowest-level Solana framework available.

The reason is PDA derivation cost. Pinocchio calls `find_program_address`, which brute-forces bump seeds from 255 downward via the `sol_create_program_address` syscall at roughly 1,500 CU per call. Quasar's `#[account(seeds = [...], bump)]` attribute generates optimized code that computes the hash via `sol_sha256` and validates via `sol_curve_validate_point`, reducing the cost to roughly 300 CU.

The Quasar vault deposit instruction is 19 lines. The Pinocchio vault deposit instruction is 77 lines. The framework code is 4x shorter and 44% cheaper.

### Scaffolding a New Project

We ran `quasar init my-program` in a fresh directory. It generated a minimal but complete project: `Cargo.toml` (cdylib crate with quasar-lang), `Quasar.toml` (project config), `src/lib.rs` (32-line no-op program), `src/tests.rs` (39-line test using quasar-svm), and a pre-generated keypair.

First build: `quasar build` produced a 2.9 KB binary in 36.7 seconds. It also auto-generated a typed Rust client crate with instruction structs that stay in sync with the program -- regenerated from the macros on every build.

First test: `quasar test` passed (1/1) in 2m 17s (dominated by first-time compilation of quasar-svm dependencies).

The pipeline works: `quasar init` to `quasar build` to `quasar test`, from zero to green.

### Writing a Custom Program

We replaced the scaffold with a counter program. Two instructions: `initialize` creates a PDA counter account seeded on the payer, `increment` bumps the count by one. The counter stores authority, count, and bump.

This is where we learned things the docs don't tell you.

The `#[account]` macro converts all fields to Pod equivalents. `u64` becomes `PodU64`, a `[u8; 8]` wrapper. You cannot assign `0` to a `PodU64`. The correct way to initialize an account is `set_inner()`, which takes native types in field order and handles Pod conversion. We found this pattern in the escrow example's `make.rs`.

Bump seeds are accessed via `ctx.bumps`, a generated struct. Not passed as instruction arguments, not computed manually. Found this in the escrow's `lib.rs`.

PodU64 arithmetic requires typed literals: `+= 1u64` works, `+= 1` does not (1 is i32), `+= 1.into()` does not (ambiguous From impls). Found this by reading the Pod crate source after compiler errors.

`cargo build` does not work. These are `#![no_std]` cdylib crates targeting SBF. Use `quasar build`.

With these lessons applied: `quasar build` compiled the counter in 4.5 seconds, producing a 7.2 KB binary. The auto-generated client produced both `InitializeInstruction` and `IncrementInstruction`.

### Testing the Custom Program

Single-instruction test (`test_initialize`) passed immediately.

Multi-instruction test (initialize then increment) failed with `IllegalOwner` at 42 CU. The program was rejecting the counter account because its owner was not the program ID. We had just created it.

We diagnosed this by reading the quasar-svm source. Added debug prints between calls:

```
Counter account after init: exists = false
Payer account after init: exists = true, lamports = 9998816800
```

The counter was never committed to the SVM store after init. The payer was. Root cause: `deconstruct_resulting_accounts` in quasar-svm's `svm.rs` iterates only the pre-execution account list when reconstructing post-execution state. Accounts created during execution (via init / system program CPI) exist in the transaction context but are not in the pre-execution list, so they are silently dropped during commit.

This is a bug. Sequential `process_instruction` calls cannot create-then-use accounts across calls.

Workaround: `process_instruction_chain` runs multiple instructions in a single atomic transaction, where instruction 2 sees accounts created by instruction 1 within the shared transaction context.

With the workaround applied: all 3 tests pass.

---

## The Verdict

### What Is Real

**The performance.** Quasar is faster than hand-written Pinocchio. Not by a small margin -- 44% cheaper on deposits, 75% cheaper on withdrawals. The optimized PDA derivation alone saves over 1,000 CU per call. A framework has no business being faster than the lowest-level alternative, but Quasar is.

**The zero-copy architecture.** 146 Miri tests validate memory safety under Tree Borrows. The Pod type system enforces alignment-1 at compile time. `no_alloc!` makes heap allocation physically impossible at runtime. This is not a claim -- it is structurally embedded in the code and verified by tooling.

**The test suite.** 692 tests, zero failures. Integration, memory safety, arithmetic, compile-time errors, and full program lifecycles. The coverage is comprehensive.

**The developer experience.** `quasar init` to `quasar build` to `quasar test` works end-to-end. The Anchor-mirror API means the learning curve is minimal. Auto-generated typed client crates are a meaningful quality-of-life feature.

### What Is Not Ready

**The test harness has a bug.** quasar-svm silently drops accounts created via CPI during `process_instruction` calls. Any program that creates accounts and then uses them in a subsequent test step will fail unless you use `process_instruction_chain`. This is a solvable bug, but it is present today and would confuse any developer who hits it.

**Windows is not supported.** The `quasar-profile` crate depends on Unix system calls. The CLI cannot be installed natively on Windows. We had to use WSL for the entire field test.

**Documentation has gaps.** 31 pages cover the what and how, but not the why. The Pod type rules (`+= 1u64`, not `+= 1`), the `set_inner()` initialization pattern, the `ctx.bumps` access pattern -- all of these required reading example source code to figure out. A developer without access to examples would hit compile errors with no guidance.

**It is version 0.0.0.** Pre-release, unaudited, built by a two-person team. The engineering quality is high, but the ecosystem maturity is not there yet.

### The Hoops

Things we had to figure out that the documentation did not explain:

1. **Pod type arithmetic** -- `+= 1u64` is correct, `+= 1` is not, `+= 1.into()` is ambiguous. Only discoverable by reading the Pod crate source or getting compiler errors.

2. **Account initialization** -- `set_inner()` takes native types in field order. The macro handles Pod conversion. Found this by reading the escrow example, not the docs.

3. **Bump access** -- `ctx.bumps` is a generated struct with a field per bump-annotated account. Not passed as instruction arguments. Found this by reading the escrow dispatch pattern.

4. **Build command** -- `cargo build` fails. `quasar build` (wrapping `cargo build-sbf`) is required. The programs are `#![no_std]` cdylib crates targeting SBF.

5. **Test harness difference** -- Scaffolded projects use quasar-svm. The repo's internal tests use mollusk-svm. Different APIs, different behavior, not documented.

6. **CPI-created account commit bug** -- Accounts created via init/CPI during `process_instruction` vanish between calls. Workaround: `process_instruction_chain`.

7. **Windows incompatibility** -- `quasar-profile` crate uses Unix system calls. Must use WSL or a Linux/macOS machine.

---

## Reproduction

To run this yourself:

```bash
# Prerequisites: Rust, cargo-build-sbf, quasar-cli
cargo install quasar-cli

# Clone and build
git clone https://github.com/AngryPacifist/quasar-field-test.git
cd quasar-field-test
quasar build
quasar test
```

Expected output: 3 tests passed (test_id, test_initialize, test_initialize_and_increment).

---

## Files

- `src/lib.rs` -- Counter program: init (PDA, set_inner, bump) + increment (PodU64, has_one)
- `src/tests.rs` -- Tests: single-instruction init + chained init-then-increment
- `Cargo.toml` -- Dependencies: quasar-lang, quasar-svm, auto-generated client
- `Quasar.toml` -- Project config: solana toolchain, quasarsvm-rust testing

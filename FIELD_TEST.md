# Taking Quasar for a Spin

**March 23, 2026**

Quasar is a Solana program framework. It claims to be faster than hand-written code, zero-copy, zero-allocation, and ergonomic enough to replace Anchor. Those are big claims for a v0.0.0 project by a two-person team. We decided to find out if any of it holds up.

## Cloning the Repo

We started by reading everything. The [repository](https://github.com/blueshift-gg/quasar) is a Cargo workspace with 13 directories and roughly 23 crates. We enumerated every file (293 total), read 189 of them line by line, and confirmed the remaining 102 via pattern sampling. Two were binaries. The codebase covers the core runtime (`lang`), procedural macros (`derive`), alignment-1 integer types (`pod`), SPL Token integration (`spl`), four-target IDL code generation (`idl`), a compute unit profiler (`profile`), the CLI (`cli`), six example programs, six test programs, and a comprehensive integration suite.

The [documentation](https://quasar-lang.com) spans 31 pages across nine sections. It covers enough to get started but stops short of explaining the internal mechanics. How the Pod types actually work, how the PDA optimization achieves its savings, how `set_inner()` handles conversion, how bumps flow through the generated code. Those answers live in the source, not the docs.

## Building Everything

We ran `make build-sbf` and compiled nine SBF binaries: three example programs (vault, escrow, multisig) and six test programs (accounts, errors, events, misc, PDA, sysvar/token CPI). We also built a raw Pinocchio vault from the repo's examples for later comparison. All ten binaries compiled cleanly.

## Running Every Test

The repo ships 692 tests. We ran all of them.

**692 passed. 0 failed.**

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

The 146 Miri tests are worth calling out. They run under Tree Borrows with symbolic alignment checking, validating memory safety for the zero-copy pointer casting that sits at the heart of the framework. All passed.

## Measuring the Compute Units

These numbers come from the test output logs. Actual SVM execution via mollusk-svm, not projections.

**Quasar Vault:** Deposit 1,576 CU. Withdraw 410 CU.
**Quasar Escrow:** Make 29,292 CU. Take 21,573 CU. Refund 8,516 CU.
**Quasar Multisig:** Create 2,013 CU. Execute transfer 3,774 CU.

A vault withdraw at 410 CU is remarkably low. For context, that covers reading a PDA, validating ownership, and transferring lamports back to the caller.

## Comparing Against Pinocchio

The repository includes a Pinocchio implementation of the same vault. Same logic, same test setup, same SVM configuration. The only difference is the framework.

| Operation | Quasar | Pinocchio | Savings |
|---|---|---|---|
| Deposit | 1,576 CU | 2,833 CU | 44% |
| Withdraw | 410 CU | 1,635 CU | 75% |

Pinocchio is the lowest-level Solana framework available. It is one step above writing raw bytes. Quasar, a macro-driven framework with account validation and auto-generated clients, produces cheaper programs.

The reason comes down to PDA derivation. Pinocchio calls `find_program_address`, which brute-forces bump seeds from 255 downward using the `sol_create_program_address` syscall. That costs roughly 1,500 CU per invocation. Quasar's `#[account(seeds = [...], bump)]` generates code that computes the hash directly via `sol_sha256` and checks the curve point via `sol_curve_validate_point`. About 300 CU.

The Quasar vault deposit is also 19 lines of Rust. The Pinocchio vault deposit is 77.

## Scaffolding a New Project

We wanted to know if the framework works when you are starting from scratch, so we ran `quasar init my-program` in a clean directory. It produced a `Cargo.toml`, a `Quasar.toml`, a 32-line program with a single no-op instruction, a 39-line test file, and a pre-generated keypair.

`quasar build` compiled the binary (2.9 KB) in 36.7 seconds. It also auto-generated a typed Rust client crate at `target/client/rust/my-program-client/`, with instruction structs derived from the program's macros. Every time the program changes and rebuilds, the client regenerates to stay in sync.

`quasar test` compiled test dependencies (2 minutes 17 seconds for the first run, mostly quasar-svm) and ran the single test. It passed. The full pipeline works: init, build, test.

## Writing a Counter Program

We replaced the scaffolded code with something that exercises real features. A counter program with two instructions: `initialize` creates a PDA account seeded on the payer, `increment` adds one to the stored count. The account stores authority, count, and bump.

It did not compile on the first try.

The `#[account]` macro converts `u64` to `PodU64`, a `[u8; 8]` wrapper that guarantees alignment 1 for zero-copy safety. You cannot assign `0` to a `PodU64` field directly. The correct approach is `set_inner()`, which takes native types in field order and handles Pod conversion. We found this by reading the escrow example's `make.rs`, because the docs do not explain it.

Bump seeds are accessed through `ctx.bumps`, a struct generated by `#[derive(Accounts)]`. They are computed by the framework during account validation. We found this pattern in the escrow's dispatcher, again because the docs do not cover it.

PodU64 arithmetic requires typed literals. `self.counter.count += 1u64` compiles. `+= 1` does not, because `1` defaults to `i32`. `+= 1.into()` does not, because `From` is implemented for too many types and the compiler cannot resolve the ambiguity. We found this by reading the Pod crate source after hitting compiler errors.

Also, `cargo build` does not work for these programs. They are `#![no_std]` cdylib crates targeting the SBF instruction set. The correct command is `quasar build`, which wraps `cargo build-sbf`.

Once we understood these patterns, the counter compiled in 4.5 seconds and produced a 7.2 KB binary. The auto-generated client included both `InitializeInstruction` and `IncrementInstruction`.

## Testing the Counter

The single-instruction test passed immediately. Initialize creates the PDA, sets the authority, stores the bump, writes count as zero.

The multi-instruction test failed. Initialize then increment, two separate `process_instruction` calls. The increment instruction rejected the counter account with `IllegalOwner` at 42 compute units.

42 CU is barely enough to read the discriminator and start account validation. The program was checking the counter's owner field, finding system program instead of our program ID, and returning an error. But we had just created the counter in the previous call. It should have been owned by our program.

We added debug prints between the two calls:

```
Counter account after init: exists = false
Payer account after init: exists = true, lamports = 9998816800
```

The payer was committed to the SVM store correctly. Lamports reduced from 10 billion, with the difference covering rent for the new account. But the counter itself was gone. The init instruction succeeded, account creation happened (we can see the lamport deduction), yet the created account was never persisted.

We traced this through quasar-svm's source. The `deconstruct_resulting_accounts` function reconstructs post-execution account state by iterating the pre-execution account list and looking each one up in the transaction context. Accounts that did not exist before execution are not in that list. Accounts created during execution via CPI (like the system program creating our counter) exist in the transaction context but are never iterated, never reconstructed, never committed back to the store.

The payer was committed because we provided it before execution. The counter was silently dropped because it was born during execution.

This is a bug in quasar-svm.

The workaround is `process_instruction_chain`, which runs multiple instructions in a single atomic transaction. Both instructions share the same transaction context, so instruction two sees accounts created by instruction one. No commit step in between, no dropped accounts.

We rewrote the test, ran it, and all three tests passed.

## What We Walked Away With

The performance is real. Quasar produces programs that cost fewer compute units than hand-written Pinocchio. The optimized PDA derivation saves over 1,000 CU per invocation, and the framework's validation overhead is lower than manual validation in raw Pinocchio. We measured this directly.

The zero-copy architecture is real. 146 Miri tests validate memory safety. The Pod type system enforces alignment at compile time. `no_alloc!` makes heap allocation impossible at runtime. These are structural properties of the code, verified by tooling.

The test coverage is thorough. 692 tests across nine suites, covering integration, memory safety, arithmetic, token operations, and compile-time error checking. All green.

The developer experience works. The pipeline from `quasar init` to `quasar build` to `quasar test` runs end to end. The Anchor-like API keeps the learning curve shallow. Auto-generated typed clients are a genuine quality-of-life feature.

But the edges are rough.

The test harness silently drops CPI-created accounts between calls. A developer hitting this would waste hours before discovering `process_instruction_chain` as the workaround, because the error message gives no indication of the actual problem.

Windows is not supported. The `quasar-profile` crate depends on Unix system calls. We ran the entire field test through WSL.

The documentation explains what things are but not how to use them in practice. Pod type arithmetic rules, the `set_inner()` initialization pattern, the `ctx.bumps` access pattern, the difference between quasar-svm and mollusk-svm. All of these required reading example source code or the framework internals. A developer without that instinct would hit walls.

It is version 0.0.0. Unaudited. Built by two people. The engineering quality is high. The ecosystem around it has not caught up yet.

---

## Reproducing This

```bash
cargo install quasar-cli
git clone https://github.com/AngryPacifist/quasar-field-test.git
cd quasar-field-test
quasar build
quasar test
```

Three tests pass: test_id, test_initialize, test_initialize_and_increment.

## Files

- `src/lib.rs` : Counter program with init and increment instructions
- `src/tests.rs` : Single-instruction and chained multi-instruction tests
- `Cargo.toml` : Dependencies (quasar-lang, quasar-svm, auto-generated client)
- `Quasar.toml` : Solana toolchain and quasarsvm-rust testing config

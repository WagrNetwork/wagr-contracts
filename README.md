# Wagr Contracts

[![Build](https://img.shields.io/badge/build-rust-orange)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-cargo-blue)](https://doc.rust-lang.org/cargo/)
[![Soroban](https://img.shields.io/badge/soroban-protocol-purple)](https://soroban.stellar.org/)

Smart contracts for skill-based wagering escrow on Soroban. Handles two-party staking, result submission, dispute resolution, and payout settlement.

## Overview

Three-contract suite implementing the core Wagr protocol:

- **escrow.rs** — Two-party stake/lock/release/timeout-refund
- **resolver.rs** — Result submission with 24h dispute window
- **payout.rs** — Winner-take-all or split settlement with fee accrual

These contracts form the trustless settlement layer for any game platform (Lichess, chess.com, manual-report, etc).

## Quick Start

### Prerequisites

- Rust 1.70+
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- Soroban CLI: `cargo install soroban-cli`

### Build

```bash
cargo build -p escrow --release
cargo build -p resolver --release
cargo build -p payout --release
```

### Test

```bash
cargo test -p escrow --features testutils
cargo test -p resolver --features testutils
cargo test -p payout --features testutils
```

### Build WASM

```bash
cargo build -p escrow --release --target wasm32-unknown-unknown
cargo build -p resolver --release --target wasm32-unknown-unknown
cargo build -p payout --release --target wasm32-unknown-unknown
```

## Contract Suite

### escrow.rs

Two-party staking pool with lock/unlock semantics.

**Key Functions:**
- `initialize(admin, fee_collector, fee_bps)` — Set up contract with admin and fee config
- `stake(player, amount, counterparty)` — Lock funds for a player (called by both players)
- `query_stake_balance(player)` — Check balance for a player
- `timeout_refund(player)` — Refund if match times out (24h+ after stake)

**State:**
- Tracks staked amounts per player per match
- Stores admin/fee collector/fee rate
- Pausable (admin can pause new stakes)

### resolver.rs

Result submission and dispute resolution with time-locked finalization.

**Key Functions:**
- `submit_result(match_id, winner, loser, proof)` — Submit signed game outcome
- `dispute(match_id, evidence)` — Challenge a submitted result
- `finalize(match_id)` — Close dispute window and finalize result
- `query_result_status(match_id)` — Check result state (pending, disputed, finalized)

**State:**
- Stores submitted results with submitter signature
- 24h dispute window per result
- Tracks disputes and evidence
- Pausable (blocks new submissions)

### payout.rs

Settlement and fee management.

**Key Functions:**
- `resolve_winner_take_all(match_id, winner)` — Transfer entire pot to winner minus fees
- `resolve_split(match_id, winner, loser, split_bps)` — Distribute pot with custom split
- `withdraw_winnings(player)` — Withdraw settled winnings
- `withdraw_fees()` — Fee collector withdraws accumulated fees
- `set_fee(new_bps)` — Admin updates fee rate

**State:**
- Tracks accumulated fees
- Stores payout status per match
- Pausable

## Architecture

```
Player 1 & 2 stake
        ↓
    Escrow locks funds
        ↓
    Game plays (off-chain)
        ↓
    Resolver submits result
        ↓
    24h dispute window
        ↓
    Payout settles winner
```

## Deployment

See [DEPLOYMENT.md](./docs/DEPLOYMENT.md) for testnet/mainnet instructions.

Quick summary:
1. Build WASM artifacts
2. Install WASM on-chain (once per version)
3. Deploy contract instances
4. Initialize with admin/fee config

## Testing

Each contract has a test module with:
- Unit tests for core functions
- Integration tests for the full flow
- Edge cases (timeout, dispute, fee calculation)

Run all tests:
```bash
cargo test --all-features
```

Run a specific contract:
```bash
cargo test -p escrow --features testutils
```

## Fee Model

Fees are in basis points (bps). 1 bps = 0.01%.

- Applied to the total pot (both players' stakes)
- Paid by the winner
- Max 1000 bps (10%)
- Admin can adjust

Example:
```
Stake: 100 XLM each (200 XLM total)
Fee: 50 bps (0.5%)
Fee amount: 200 × 50 / 10000 = 1 XLM
Winner gets: 200 - 1 = 199 XLM
```

## State Machine

```
Uninitialized → initialize() → Initialized/Active
Active → pause() → Paused
Paused → unpause() → Active
Active → upgrade() → Active (new code)
```

## Security Considerations

- Admin keypair must be stored in a secrets manager (AWS Secrets Manager, Vault, etc.)
- Fee collector keypair separate from admin
- Consider multisig for admin key
- All transactions require explicit signatures
- Dispute window is non-negotiable (24h default)
- Results are immutable once finalized

## Gas & Performance

- Escrow operations: ~1000 ops
- Result submission: ~2000 ops
- Payout settlement: ~1500 ops
- Estimated cost per match: 0.05–0.1 XLM

## Contributing

Issues and PRs welcome. See [CONTRIBUTING.md](./CONTRIBUTING.md) for setup.

## License

MIT

## Related

- [wagr-sdk](https://github.com/stellar/wagr-sdk) — TypeScript SDK for adapters and match orchestration
- [wagr-app](https://github.com/stellar/wagr-app) — Frontend for match lobby and staking UI

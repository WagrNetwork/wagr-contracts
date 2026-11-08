# Wagr Contracts

[![CI](https://github.com/WagrNetwork/wagr-contracts/actions/workflows/ci.yml/badge.svg)](https://github.com/WagrNetwork/wagr-contracts/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)

Soroban smart contracts for **trustless escrow, result attestation and settlement of competitive matches on Stellar**.

Two or more parties lock funds in escrow, a registered oracle reports the outcome of an off-chain event (a chess game, an esports match, a tournament round, a coding challenge), the losing side gets a dispute window, and the pot is settled on-chain. No party, including the operator, can move funds outside those rules.

Typical uses:

- Tournament prize pools and entry fees held in escrow until results are final
- Head-to-head challenges between players or teams, settled in USDC or XLM
- Any two-party agreement whose outcome is reported by an oracle and can be disputed

## Contracts

| Crate | Role |
| --- | --- |
| `escrow` | Holds stakes per match (2–8 parties). One asset per match, fixed by the first stake. Timeout refunds after 24h. Releases the pot only to the configured settler. |
| `resolver` | Registered oracles submit results. The recorded loser can dispute within 24h. Undisputed results finalize; disputed ones are decided by the admin. |
| `payout` | Pulls the pot from escrow, takes a capped protocol fee, and records winner-take-all or split payouts that each party withdraws. |

### Flow

```
parties stake ──► escrow (funds locked, one asset per match)
                     │
oracle reports ──► resolver ── 24h dispute window ──► finalized result
                                                         │
arbiter settles ──► payout ── escrow.release() ──► pot moved to payout
                                                         │
                     parties withdraw winnings, fee collector withdraws fees
```

### Safety properties covered by tests

- A match cannot mix assets, and refunds/withdrawals are only paid in the asset that was staked.
- Settlement pulls the pot from escrow, so the arbiter cannot settle a pot larger than what was actually staked.
- A pot can be released once; after release no stake can be refunded.
- Only registered oracles can submit results; a disputed result cannot auto-finalize.
- Double settlement and double withdrawal are rejected.

## Development

Prerequisites: Rust stable, the `wasm32v1-none` target, and [Stellar CLI](https://developers.stellar.org/docs/tools/cli) v25.2+ (soroban-sdk 29 requires it for WASM builds).

```bash
rustup target add wasm32v1-none
cargo test               # run all contract tests
stellar contract build   # build the three .wasm files
```

The `.wasm` files are written to `target/wasm32v1-none/release/`.

### Deploying to testnet

With the [Stellar CLI](https://developers.stellar.org/docs/tools/cli) and a funded testnet identity:

```bash
for c in escrow resolver payout; do
  stellar contract deploy --wasm target/wasm32v1-none/release/$c.wasm \
    --source <identity> --network testnet
done
```

Then initialize and wire the contracts together:

1. `escrow.initialize(admin, fee_collector, fee_bps)`, `resolver.initialize(admin)`, `payout.initialize(admin, fee_collector, fee_bps)`
2. `escrow.set_settler(<payout contract id>)` and `payout.set_escrow(<escrow contract id>)`
3. `resolver.set_oracle(<oracle address>, true)` for each result reporter
4. `payout.set_arbiter(<settlement operator>)`

## Roadmap

Open work is tracked in [issues](https://github.com/WagrNetwork/wagr-contracts/issues). Highlights:

- Migrate events to typed `#[contractevent]` structs
- Let payout verify the resolver's finalized result on-chain instead of trusting the arbiter's winner
- Multi-party (N-way) payout splits
- Publish testnet deployment addresses

## Related

- [wagr-sdk](https://github.com/WagrNetwork/wagr-sdk): TypeScript SDK and result adapters (Lichess)
- [wagr-app](https://github.com/WagrNetwork/wagr-app): reference web app

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md). Licensed under [MIT](./LICENSE).

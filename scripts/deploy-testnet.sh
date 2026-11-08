#!/usr/bin/env bash
# Deploy escrow, resolver and payout to Stellar testnet, wire them together,
# run a live smoke test, then hand control to $ADMIN_ADDRESS (if given).
#
# Requirements: stellar-cli v25.2+, jq. Uses friendbot, so testnet only.
# Writes deployments/testnet.json.
set -euo pipefail

NET="--network testnet"
FEE_BPS="${FEE_BPS:-50}"
WASM=target/wasm32v1-none/release

log() { echo "::group::$*" 2>/dev/null || echo "== $*"; }
end() { echo "::endgroup::" 2>/dev/null || true; }
inv() { local id=$1 src=$2; shift 2; stellar contract invoke --id "$id" --source "$src" $NET -- "$@"; }

log "Identities"
for k in deployer player1 player2; do
  stellar keys generate "$k" $NET --fund --overwrite
done
DEPLOYER=$(stellar keys address deployer)
P1=$(stellar keys address player1)
P2=$(stellar keys address player2)
XLM=$(stellar contract id asset --asset native $NET)
echo "deployer=$DEPLOYER xlm=$XLM"
end

log "Deploy"
ESCROW=$(stellar contract deploy --wasm $WASM/escrow.wasm --source deployer $NET)
RESOLVER=$(stellar contract deploy --wasm $WASM/resolver.wasm --source deployer $NET)
PAYOUT=$(stellar contract deploy --wasm $WASM/payout.wasm --source deployer $NET)
echo "escrow=$ESCROW resolver=$RESOLVER payout=$PAYOUT"
end

log "Initialize and wire"
inv "$ESCROW" deployer initialize --admin "$DEPLOYER" --fee_collector "$DEPLOYER" --fee_bps "$FEE_BPS"
inv "$RESOLVER" deployer initialize --admin "$DEPLOYER"
inv "$PAYOUT" deployer initialize --admin "$DEPLOYER" --fee_collector "$DEPLOYER" --fee_bps "$FEE_BPS"
inv "$ESCROW" deployer set_settler --settler "$PAYOUT"
inv "$PAYOUT" deployer set_escrow --escrow "$ESCROW"
inv "$RESOLVER" deployer set_oracle --oracle "$DEPLOYER" --enabled true
end

log "Smoke test: stake -> report -> settle -> withdraw"
MATCH="smoke_$(date +%s)"
STAKE=10000000 # 1 XLM in stroops
inv "$ESCROW" player1 stake --match_id "$MATCH" --player "$P1" --amount $STAKE --asset "$XLM"
inv "$ESCROW" player2 stake --match_id "$MATCH" --player "$P2" --amount $STAKE --asset "$XLM"
POT=$(inv "$ESCROW" deployer get_total_escrow | tr -d '"')
test "$POT" = "$((STAKE * 2))" || { echo "unexpected escrow total $POT"; exit 1; }
inv "$RESOLVER" deployer submit_result --match_id "$MATCH" --winner "$P1" --loser "$P2" \
  --reason checkmate --proof 00 --submitter "$DEPLOYER"
inv "$PAYOUT" deployer resolve_winner_take_all --match_id "$MATCH" --winner "$P1" \
  --total_pot $((STAKE * 2)) --asset "$XLM"
WON=$(inv "$PAYOUT" player1 withdraw_winnings --match_id "$MATCH" --player "$P1" --asset "$XLM" | tr -d '"')
EXPECTED=$((STAKE * 2 - STAKE * 2 * FEE_BPS / 10000))
test "$WON" = "$EXPECTED" || { echo "winner got $WON, expected $EXPECTED"; exit 1; }
test "$(inv "$ESCROW" deployer get_total_escrow | tr -d '"')" = "0"
echo "smoke test passed: winner withdrew $WON stroops"
end

OWNER="$DEPLOYER"
if [ -n "${ADMIN_ADDRESS:-}" ]; then
  log "Hand over control to $ADMIN_ADDRESS"
  inv "$RESOLVER" deployer set_oracle --oracle "$ADMIN_ADDRESS" --enabled true
  inv "$RESOLVER" deployer set_oracle --oracle "$DEPLOYER" --enabled false
  inv "$PAYOUT" deployer set_arbiter --new_arbiter "$ADMIN_ADDRESS"
  inv "$PAYOUT" deployer set_fee_collector --new_collector "$ADMIN_ADDRESS"
  inv "$ESCROW" deployer set_fee_collector --new_collector "$ADMIN_ADDRESS"
  for c in "$ESCROW" "$RESOLVER" "$PAYOUT"; do
    inv "$c" deployer set_admin --new_admin "$ADMIN_ADDRESS"
  done
  OWNER="$ADMIN_ADDRESS"
  end
fi

mkdir -p deployments
jq -n --arg escrow "$ESCROW" --arg resolver "$RESOLVER" --arg payout "$PAYOUT" \
  --arg xlm "$XLM" --arg owner "$OWNER" --arg commit "${GITHUB_SHA:-local}" \
  --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg match "$MATCH" \
  '{network:"testnet", rpcUrl:"https://soroban-testnet.stellar.org",
    networkPassphrase:"Test SDF Network ; September 2015",
    contracts:{escrow:$escrow, resolver:$resolver, payout:$payout},
    nativeAsset:$xlm, admin:$owner, smokeTestMatch:$match,
    deployedAt:$date, commit:$commit}' > deployments/testnet.json
cat deployments/testnet.json

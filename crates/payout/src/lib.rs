#![no_std]
// Events still use `env.events().publish`; migrating to `#[contractevent]` types is tracked as an open issue.
#![allow(deprecated)]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, Address, BytesN, Env,
    Symbol,
};

/// The part of the escrow contract's interface the payout contract calls.
#[contractclient(name = "EscrowClient")]
pub trait EscrowInterface {
    fn release(env: Env, match_id: Symbol, recipient: Address) -> (Address, u128);
}

const BUMP_THRESHOLD: u32 = 518_400; // ~30 days at 5s/ledger
const BUMP_EXTEND_TO: u32 = 535_680; // ~31 days at 5s/ledger

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[contracterror]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidFee = 3,
    Paused = 4,
    AlreadySettled = 5,
    InvalidSplit = 6,
    NoWinnings = 7,
    AlreadyWithdrawn = 8,
    NoFees = 9,
    Unauthorized = 10,
    EscrowNotSet = 11,
    PotMismatch = 12,
    AssetMismatch = 13,
    SamePlayer = 14,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[contracttype]
pub enum DataKey {
    Admin,
    FeeCollector,
    Arbiter, // Only the arbiter (the resolver/settlement authority) can settle matches
    FeeBps,
    IsPaused,
    /// Escrow contract that holds the stakes this contract settles.
    Escrow,
}

#[contract]
pub struct PayoutContract;

fn fee_balance_key(env: &Env, asset: &Address) -> (Symbol, Address) {
    (Symbol::new(env, "feebal"), asset.clone())
}

fn settled_key(env: &Env, match_id: &Symbol) -> (Symbol, Symbol) {
    (Symbol::new(env, "settled"), match_id.clone())
}

fn winnings_key(env: &Env, match_id: &Symbol, player: &Address) -> (Symbol, Symbol, Address) {
    (
        Symbol::new(env, "winnings"),
        match_id.clone(),
        player.clone(),
    )
}

fn withdrawn_key(env: &Env, match_id: &Symbol, player: &Address) -> (Symbol, Symbol, Address) {
    (
        Symbol::new(env, "withdrawn"),
        match_id.clone(),
        player.clone(),
    )
}

fn match_asset_key(env: &Env, match_id: &Symbol) -> (Symbol, Symbol) {
    (Symbol::new(env, "masset"), match_id.clone())
}

/// Pull the match pot out of escrow into this contract and check it matches
/// what the arbiter claims. Settlement can never pay out more than was staked,
/// or in a different asset.
fn pull_pot(env: &Env, match_id: &Symbol, total_pot: u128, asset: &Address) -> Result<(), Error> {
    let escrow: Address = env
        .storage()
        .instance()
        .get(&DataKey::Escrow)
        .ok_or(Error::EscrowNotSet)?;
    let (released_asset, pot) =
        EscrowClient::new(env, &escrow).release(match_id, &env.current_contract_address());
    if released_asset != *asset {
        return Err(Error::AssetMismatch);
    }
    if pot != total_pot {
        return Err(Error::PotMismatch);
    }
    let key = match_asset_key(env, match_id);
    env.storage().persistent().set(&key, asset);
    env.storage()
        .persistent()
        .extend_ttl(&key, BUMP_THRESHOLD, BUMP_EXTEND_TO);
    Ok(())
}

fn require_admin(env: &Env) -> Result<Address, Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)?;
    admin.require_auth();
    Ok(admin)
}

fn require_arbiter(env: &Env) -> Result<Address, Error> {
    let arbiter: Address = env
        .storage()
        .instance()
        .get(&DataKey::Arbiter)
        .ok_or(Error::NotInitialized)?;
    arbiter.require_auth();
    Ok(arbiter)
}

fn require_not_paused(env: &Env) -> Result<(), Error> {
    let is_paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::IsPaused)
        .unwrap_or(false);
    if is_paused {
        return Err(Error::Paused);
    }
    Ok(())
}

#[contractimpl]
impl PayoutContract {
    /// Initialize the payout contract. The admin is also set as the initial
    /// arbiter (the only party authorized to settle matches); change it with
    /// `set_arbiter` once a dedicated resolver address is known.
    pub fn initialize(
        env: Env,
        admin: Address,
        fee_collector: Address,
        fee_bps: u32,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        admin.require_auth();

        if fee_bps > 1000 {
            return Err(Error::InvalidFee);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Arbiter, &admin);
        env.storage()
            .instance()
            .set(&DataKey::FeeCollector, &fee_collector);
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        env.storage().instance().set(&DataKey::IsPaused, &false);

        Ok(())
    }

    /// Settle a match with winner-take-all payout (deduct fees from pot).
    /// Only callable by the arbiter (the trusted settlement authority, e.g.
    /// the resolver contract's operator) once a result has been finalized.
    pub fn resolve_winner_take_all(
        env: Env,
        match_id: Symbol,
        winner: Address,
        total_pot: u128,
        asset: Address,
    ) -> Result<(), Error> {
        require_arbiter(&env)?;
        require_not_paused(&env)?;

        let skey = settled_key(&env, &match_id);
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&skey)
            .unwrap_or(false)
        {
            return Err(Error::AlreadySettled);
        }

        pull_pot(&env, &match_id, total_pot, &asset)?;

        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);
        let fee: u128 = (total_pot * fee_bps as u128) / 10000;
        let winner_amount = total_pot - fee;

        let fbkey = fee_balance_key(&env, &asset);
        let current_fee_balance: u128 = env.storage().persistent().get(&fbkey).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&fbkey, &(current_fee_balance + fee));

        let wkey = winnings_key(&env, &match_id, &winner);
        env.storage().persistent().set(&wkey, &winner_amount);
        env.storage()
            .persistent()
            .extend_ttl(&wkey, BUMP_THRESHOLD, BUMP_EXTEND_TO);

        env.storage().persistent().set(&skey, &true);

        env.events().publish(
            ("payout", "settled", match_id),
            (winner, winner_amount, fee),
        );

        Ok(())
    }

    /// Settle a match with a custom split (e.g., 70/30 split) between winner
    /// and loser. Only callable by the arbiter.
    pub fn resolve_split(
        env: Env,
        match_id: Symbol,
        winner: Address,
        loser: Address,
        total_pot: u128,
        winner_split_bps: u32, // e.g., 7000 for 70%
        asset: Address,
    ) -> Result<(), Error> {
        require_arbiter(&env)?;
        require_not_paused(&env)?;

        let skey = settled_key(&env, &match_id);
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&skey)
            .unwrap_or(false)
        {
            return Err(Error::AlreadySettled);
        }

        if winner_split_bps > 10000 {
            return Err(Error::InvalidSplit);
        }
        if winner == loser {
            return Err(Error::SamePlayer);
        }

        pull_pot(&env, &match_id, total_pot, &asset)?;

        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);
        let fee: u128 = (total_pot * fee_bps as u128) / 10000;
        let net_pot = total_pot - fee;

        let winner_amount = (net_pot * winner_split_bps as u128) / 10000;
        let loser_amount = net_pot - winner_amount;

        let fbkey = fee_balance_key(&env, &asset);
        let current_fee_balance: u128 = env.storage().persistent().get(&fbkey).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&fbkey, &(current_fee_balance + fee));

        let wkey = winnings_key(&env, &match_id, &winner);
        let lkey = winnings_key(&env, &match_id, &loser);
        env.storage().persistent().set(&wkey, &winner_amount);
        env.storage().persistent().set(&lkey, &loser_amount);
        env.storage()
            .persistent()
            .extend_ttl(&wkey, BUMP_THRESHOLD, BUMP_EXTEND_TO);
        env.storage()
            .persistent()
            .extend_ttl(&lkey, BUMP_THRESHOLD, BUMP_EXTEND_TO);

        env.storage().persistent().set(&skey, &true);

        env.events().publish(
            ("payout", "split_settled", match_id),
            (winner, winner_amount, loser, loser_amount, fee),
        );

        Ok(())
    }

    /// Withdraw winnings for a player.
    pub fn withdraw_winnings(
        env: Env,
        match_id: Symbol,
        player: Address,
        asset: Address,
    ) -> Result<u128, Error> {
        player.require_auth();

        let wkey = winnings_key(&env, &match_id, &player);
        let amount: u128 = env
            .storage()
            .persistent()
            .get(&wkey)
            .ok_or(Error::NoWinnings)?;

        let dkey = withdrawn_key(&env, &match_id, &player);
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&dkey)
            .unwrap_or(false)
        {
            return Err(Error::AlreadyWithdrawn);
        }

        // Winnings are paid only in the asset the match was settled in.
        let settled_asset: Address = env
            .storage()
            .persistent()
            .get(&match_asset_key(&env, &match_id))
            .ok_or(Error::NoWinnings)?;
        if settled_asset != asset {
            return Err(Error::AssetMismatch);
        }

        let token = soroban_sdk::token::Client::new(&env, &asset);
        token.transfer(&env.current_contract_address(), &player, &(amount as i128));

        env.storage().persistent().set(&dkey, &true);

        env.events()
            .publish(("payout", "withdrawn", match_id), (player, amount));

        Ok(amount)
    }

    /// Point this contract at the escrow contract it settles from (admin only).
    /// The escrow contract must in turn name this contract as its settler.
    pub fn set_escrow(env: Env, escrow: Address) -> Result<(), Error> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::Escrow, &escrow);
        Ok(())
    }

    /// Withdraw accumulated fees (fee collector only).
    pub fn withdraw_fees(env: Env, asset: Address) -> Result<u128, Error> {
        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(Error::NotInitialized)?;
        fee_collector.require_auth();

        let fbkey = fee_balance_key(&env, &asset);
        let fee_balance: u128 = env.storage().persistent().get(&fbkey).unwrap_or(0);

        if fee_balance == 0 {
            return Err(Error::NoFees);
        }

        let token = soroban_sdk::token::Client::new(&env, &asset);
        token.transfer(
            &env.current_contract_address(),
            &fee_collector,
            &(fee_balance as i128),
        );

        env.storage().persistent().set(&fbkey, &0u128);

        env.events()
            .publish(("fee", "withdrawn"), (asset, fee_balance));

        Ok(fee_balance)
    }

    /// Get fee balance for an asset.
    pub fn get_fee_balance(env: Env, asset: Address) -> u128 {
        let fbkey = fee_balance_key(&env, &asset);
        env.storage().persistent().get(&fbkey).unwrap_or(0)
    }

    /// Set fee rate (admin only).
    pub fn set_fee_bps(env: Env, new_fee_bps: u32) -> Result<(), Error> {
        require_admin(&env)?;
        if new_fee_bps > 1000 {
            return Err(Error::InvalidFee);
        }
        env.storage().instance().set(&DataKey::FeeBps, &new_fee_bps);
        Ok(())
    }

    /// Set fee collector (admin only).
    pub fn set_fee_collector(env: Env, new_collector: Address) -> Result<(), Error> {
        require_admin(&env)?;
        env.storage()
            .instance()
            .set(&DataKey::FeeCollector, &new_collector);
        Ok(())
    }

    /// Set the arbiter, the sole authority permitted to settle matches (admin only).
    pub fn set_arbiter(env: Env, new_arbiter: Address) -> Result<(), Error> {
        require_admin(&env)?;
        env.storage()
            .instance()
            .set(&DataKey::Arbiter, &new_arbiter);
        Ok(())
    }

    /// Set admin (admin only).
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    /// Pause the contract (admin only).
    pub fn pause(env: Env) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        env.storage().instance().set(&DataKey::IsPaused, &true);
        env.events().publish(("contract", "paused"), admin);
        Ok(())
    }

    /// Unpause the contract (admin only).
    pub fn unpause(env: Env) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        env.storage().instance().set(&DataKey::IsPaused, &false);
        env.events().publish(("contract", "unpaused"), admin);
        Ok(())
    }

    /// Check whether the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::IsPaused)
            .unwrap_or(false)
    }

    /// Upgrade the contract's WASM code (admin only).
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), Error> {
        require_admin(&env)?;
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use escrow::{EscrowContract, EscrowContractClient};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{token, Env};

    struct Setup<'a> {
        env: Env,
        payout: PayoutContractClient<'a>,
        escrow: EscrowContractClient<'a>,
        asset: Address,
        p1: Address,
        p2: Address,
        fee_collector: Address,
    }

    /// Escrow + payout wired together, a token, and a 2-player match "m1"
    /// with 1_000 staked by each player.
    fn setup<'a>() -> Setup<'a> {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);

        let escrow_id = env.register(EscrowContract, ());
        let payout_id = env.register(PayoutContract, ());
        let escrow = EscrowContractClient::new(&env, &escrow_id);
        let payout = PayoutContractClient::new(&env, &payout_id);
        escrow.initialize(&admin, &fee_collector, &50);
        payout.initialize(&admin, &fee_collector, &50);
        escrow.set_settler(&payout_id);
        payout.set_escrow(&escrow_id);

        let asset = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();
        let sac = token::StellarAssetClient::new(&env, &asset);
        let p1 = Address::generate(&env);
        let p2 = Address::generate(&env);
        sac.mint(&p1, &10_000);
        sac.mint(&p2, &10_000);

        let m = Symbol::new(&env, "m1");
        escrow.stake(&m, &p1, &1_000, &asset);
        escrow.stake(&m, &p2, &1_000, &asset);

        Setup {
            env,
            payout,
            escrow,
            asset,
            p1,
            p2,
            fee_collector,
        }
    }

    #[test]
    fn test_winner_take_all_end_to_end() {
        let s = setup();
        let m = Symbol::new(&s.env, "m1");
        let token = token::Client::new(&s.env, &s.asset);

        s.payout
            .resolve_winner_take_all(&m, &s.p1, &2_000, &s.asset);
        assert_eq!(s.escrow.get_total_escrow(), 0);

        // 0.5% fee on a 2_000 pot.
        assert_eq!(s.payout.get_fee_balance(&s.asset), 10);
        assert_eq!(s.payout.withdraw_winnings(&m, &s.p1, &s.asset), 1_990);
        assert_eq!(token.balance(&s.p1), 10_000 - 1_000 + 1_990);
        assert_eq!(s.payout.withdraw_fees(&s.asset), 10);
        assert_eq!(token.balance(&s.fee_collector), 10);
        assert_eq!(token.balance(&s.payout.address), 0);
    }

    #[test]
    fn test_split_end_to_end() {
        let s = setup();
        let m = Symbol::new(&s.env, "m1");
        s.payout
            .resolve_split(&m, &s.p1, &s.p2, &2_000, &7_000, &s.asset);
        assert_eq!(s.payout.withdraw_winnings(&m, &s.p1, &s.asset), 1_393);
        assert_eq!(s.payout.withdraw_winnings(&m, &s.p2, &s.asset), 597);
    }

    #[test]
    fn test_arbiter_cannot_inflate_pot() {
        let s = setup();
        let m = Symbol::new(&s.env, "m1");
        let res = s
            .payout
            .try_resolve_winner_take_all(&m, &s.p1, &5_000, &s.asset);
        assert!(res.is_err());
        // Nothing moved: the stakes are still in escrow.
        assert_eq!(s.escrow.get_total_escrow(), 2_000);
    }

    #[test]
    fn test_withdraw_rejects_other_asset() {
        let s = setup();
        let m = Symbol::new(&s.env, "m1");
        s.payout
            .resolve_winner_take_all(&m, &s.p1, &2_000, &s.asset);
        let other = s
            .env
            .register_stellar_asset_contract_v2(Address::generate(&s.env))
            .address();
        let res = s.payout.try_withdraw_winnings(&m, &s.p1, &other);
        assert_eq!(res, Err(Ok(Error::AssetMismatch)));
    }

    #[test]
    fn test_double_settle_and_double_withdraw_rejected() {
        let s = setup();
        let m = Symbol::new(&s.env, "m1");
        s.payout
            .resolve_winner_take_all(&m, &s.p1, &2_000, &s.asset);
        assert_eq!(
            s.payout
                .try_resolve_winner_take_all(&m, &s.p1, &2_000, &s.asset),
            Err(Ok(Error::AlreadySettled))
        );
        s.payout.withdraw_winnings(&m, &s.p1, &s.asset);
        assert_eq!(
            s.payout.try_withdraw_winnings(&m, &s.p1, &s.asset),
            Err(Ok(Error::AlreadyWithdrawn))
        );
    }

    #[test]
    fn test_settle_requires_escrow() {
        let env = Env::default();
        env.mock_all_auths();
        let payout = PayoutContractClient::new(&env, &env.register(PayoutContract, ()));
        payout.initialize(&Address::generate(&env), &Address::generate(&env), &50);
        let res = payout.try_resolve_winner_take_all(
            &Symbol::new(&env, "m1"),
            &Address::generate(&env),
            &100,
            &Address::generate(&env),
        );
        assert_eq!(res, Err(Ok(Error::EscrowNotSet)));
    }
}

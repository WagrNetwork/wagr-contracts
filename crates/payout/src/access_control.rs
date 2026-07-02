// Feature 6: Admin Access Control
// Role-based access control for admin, fee collector, and arbiter

use soroban_sdk::{Address, Env, Symbol};

#[derive(Clone)]
pub struct AdminRole {
    pub admin: Address,
    pub fee_collector: Address,
    pub arbiter: Address,
}

pub enum AccessLevel {
    Admin,
    FeeCollector,
    Arbiter,
    None,
}

pub fn check_admin_access(env: &Env, caller: &Address) -> Result<(), Symbol> {
    let admin: Address = env.storage()
        .instance()
        .get(&"admin")
        .ok_or(Symbol::short("no_admin"))?;

    if admin != *caller {
        return Err(Symbol::short("unauthorized"));
    }

    Ok(())
}

pub fn check_fee_collector_access(env: &Env, caller: &Address) -> Result<(), Symbol> {
    let fee_collector: Address = env.storage()
        .instance()
        .get(&"fee_collector")
        .ok_or(Symbol::short("no_collector"))?;

    if fee_collector != *caller {
        return Err(Symbol::short("unauthorized"));
    }

    Ok(())
}

pub fn check_arbiter_access(env: &Env, caller: &Address) -> Result<(), Symbol> {
    let arbiter: Address = env.storage()
        .instance()
        .get(&"arbiter")
        .ok_or(Symbol::short("no_arbiter"))?;

    if arbiter != *caller {
        return Err(Symbol::short("unauthorized"));
    }

    Ok(())
}

pub fn get_access_level(env: &Env, caller: &Address) -> AccessLevel {
    if check_admin_access(env, caller).is_ok() {
        return AccessLevel::Admin;
    }

    if check_fee_collector_access(env, caller).is_ok() {
        return AccessLevel::FeeCollector;
    }

    if check_arbiter_access(env, caller).is_ok() {
        return AccessLevel::Arbiter;
    }

    AccessLevel::None
}

pub fn transfer_admin_role(
    env: &Env,
    caller: &Address,
    new_admin: Address,
) -> Result<(), Symbol> {
    check_admin_access(env, caller)?;

    env.storage()
        .instance()
        .set(&"admin", &new_admin);

    env.events().publish(
        ("admin", "role_transferred"),
        (caller, new_admin),
    );

    Ok(())
}

pub fn set_fee_collector(
    env: &Env,
    caller: &Address,
    new_collector: Address,
) -> Result<(), Symbol> {
    check_admin_access(env, caller)?;

    env.storage()
        .instance()
        .set(&"fee_collector", &new_collector);

    env.events().publish(
        ("admin", "fee_collector_updated"),
        (caller, new_collector),
    );

    Ok(())
}

pub fn set_arbiter(
    env: &Env,
    caller: &Address,
    new_arbiter: Address,
) -> Result<(), Symbol> {
    check_admin_access(env, caller)?;

    env.storage()
        .instance()
        .set(&"arbiter", &new_arbiter);

    env.events().publish(
        ("admin", "arbiter_updated"),
        (caller, new_arbiter),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_level_enum() {
        let level = AccessLevel::Admin;
        match level {
            AccessLevel::Admin => assert!(true),
            _ => panic!("Expected Admin"),
        }
    }
}

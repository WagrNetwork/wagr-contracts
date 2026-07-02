use soroban_sdk::{Address, Env, Symbol};

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&crate::DataKey::Admin)
}

pub fn set_admin(env: &Env, admin: Address) {
    env.storage().instance().set(&crate::DataKey::Admin, &admin);
}

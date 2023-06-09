#![no_std]

mod pool_contract;
mod test;
pub mod testutils;

use pool_contract::LiquidityPoolClient;
use soroban_sdk::{contractimpl, contracttype, Address, Bytes, BytesN, Env};
use soroban_sdk::xdr::ToXdr;

mod token {
    // soroban_sdk::contractimport!(file = "../soroban_token_spec.wasm");

    soroban_sdk::contractimport!(
        file = "../token/target/wasm32-unknown-unknown/release/soroban_token_contract.wasm"
    );
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Pool(BytesN<32>),
}

fn get_pool_id(e: &Env, salt: &BytesN<32>) -> Address {
    e.storage()
        .get_unchecked(&DataKey::Pool(salt.clone()))
        .unwrap()
}

fn put_pool(e: &Env, salt: &BytesN<32>, pool: &Address) {
    e.storage().set(&DataKey::Pool(salt.clone()), pool)
}

fn has_pool(e: &Env, salt: &BytesN<32>) -> bool {
    e.storage().has(&DataKey::Pool(salt.clone()))
}

pub trait LiquidityPoolRouterTrait {
    fn init_pool(
        e: Env,
        liquidity_pool_wasm_hash: BytesN<32>,
        token_wasm_hash: BytesN<32>,
        token_a: Address,
        token_b: Address,
    );
    fn sf_deposit(
        e: Env,
        account: Address,
        token_a: Address,
        token_b: Address,
        desired_a: i128,
        min_a: i128,
        desired_b: i128,
        min_b: i128,
    );

    // swaps out an exact amount of "buy", in exchange for "sell" that this contract has an
    // allowance for from "to". "sell" amount swapped in must not be greater than "in_max"
    fn swap_out(e: Env, account: Address, sell: Address, buy: Address, out: i128, in_max: i128);

    fn sf_withdrw(
        e: Env,
        account: Address,
        token_a: Address,
        token_b: Address,
        share_amount: i128,
        min_a: i128,
        min_b: i128,
    );

    // returns the contract address for the specified token_a/token_b combo
    fn get_pool(e: Env, token_a: Address, token_b: Address) -> Address;
}

fn sort(a: &Address, b: &Address) -> (Address, Address) {
    if a < b {
        return (a.clone(), b.clone());
    } else if a > b {
        return (b.clone(), a.clone());
    }
    panic!("a and b can't be the same")
}

pub fn pool_salt(e: &Env, token_a: &Address, token_b: &Address) -> BytesN<32> {
    if token_a >= token_b {
        panic!("token_a must be less t&han token_b");
    }

    let mut salt = Bytes::new(e);
    salt.append(&token_a.to_xdr(e));
    salt.append(&token_b.to_xdr(e));
    e.crypto().sha256(&salt)
}

fn get_deposit_amounts(
    desired_a: i128,
    min_a: i128,
    desired_b: i128,
    min_b: i128,
    reserves: (i128, i128),
) -> (i128, i128) {
    if reserves.0 == 0 && reserves.1 == 0 {
        return (desired_a, desired_b);
    }

    let amount_b = desired_a * reserves.1 / reserves.0;
    if amount_b <= desired_b {
        if amount_b < min_b {
            panic!("amount_b less than min")
        }
        (desired_a, amount_b)
    } else {
        let amount_a = desired_b * reserves.0 / reserves.1;
        if amount_a > desired_a || desired_a < min_a {
            panic!("amount_a invalid")
        }
        (amount_a, desired_b)
    }
}

struct LiquidityPoolRouter;

#[contractimpl]
impl LiquidityPoolRouterTrait for LiquidityPoolRouter {
    fn init_pool(
        e: Env,
        liquidity_pool_wasm_hash: BytesN<32>,
        token_wasm_hash: BytesN<32>,
        token_a: Address,
        token_b: Address,
    ) {
        let salt = pool_salt(&e, &token_a, &token_b);
        if !has_pool(&e, &salt) {
            let pool_contract_id = e
                .deployer()
                .with_current_contract(&salt)
                .deploy(&liquidity_pool_wasm_hash);

            put_pool(&e, &salt, &pool_contract_id);

            LiquidityPoolClient::new(&e, &pool_contract_id).initialize(
                &token_wasm_hash,
                &token_a,
                &token_b,
            );
        }
    }

    fn sf_deposit(
        e: Env,
        account: Address,
        token_a: Address,
        token_b: Address,
        desired_a: i128,
        min_a: i128,
        desired_b: i128,
        min_b: i128,
    ) {
        account.require_auth();

        let salt = pool_salt(&e, &token_a, &token_b);
        let pool_id = get_pool_id(&e, &salt);

        LiquidityPoolClient::new(&e, &pool_id).deposit(
            &account,
            &desired_a,
            &min_a,
            &desired_b,
            &min_b,
        );
    }

    fn swap_out(e: Env,
        account: Address, sell: Address, buy: Address, out: i128, in_max: i128) {
        account.require_auth();
        let (token_a, token_b) = sort(&sell, &buy);
        let pool_id = Self::get_pool(e.clone(), token_a.clone(), token_b);

        LiquidityPoolClient::new(&e, &pool_id).swap(&account, &(buy == token_a), &out, &in_max)
    }

    fn sf_withdrw(
        e: Env,
        account: Address,
        token_a: Address,
        token_b: Address,
        share_amount: i128,
        min_a: i128,
        min_b: i128,
    ) {
        account.require_auth();
        let pool_id = Self::get_pool(e.clone(), token_a, token_b);

        let pool_client = LiquidityPoolClient::new(&e, &pool_id);

        let (amount_a, amount_b) = pool_client.withdraw(&account, &share_amount, &min_a, &min_b);

        if amount_a < min_a || amount_b < min_b {
            panic!("min not satisfied");
        }
    }

    fn get_pool(e: Env, token_a: Address, token_b: Address) -> Address {
        let salt = pool_salt(&e, &token_a, &token_b);
        get_pool_id(&e, &salt)
    }
}
#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    Address, Env, Map, Symbol, Vec, token,
};

mod error;
mod storage;
mod matching;
mod test;

use error::OrderBookError;
use storage::*;
use matching::*;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Order {
    pub id: u64,
    pub trader: Address,
    pub order_type: OrderType,
    pub price: u64,
    pub amount: u128,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OrderType {
    Bid = 0,
    Ask = 1,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderBookSide {
    pub orders: Map<u64, Order>,
    pub price_levels: Map<u64, Vec<u64>>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub admin: Address,
    pub base_token: Address,
    pub quote_token: Address,
    pub next_order_id: u64,
    pub fee_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Config,
    Bids,
    Asks,
    Order(u64),
}

#[contract]
pub struct LimitOrderBook;

#[contractimpl]
impl LimitOrderBook {
    pub fn initialize(
        env: Env,
        admin: Address,
        base_token: Address,
        quote_token: Address,
    ) {
        admin.require_auth();

        if env.storage().instance().has(&DataKey::Config) {
            panic!("Already initialized");
        }

        let config = Config {
            admin: admin.clone(),
            base_token: base_token.clone(),
            quote_token: quote_token.clone(),
            next_order_id: 1,
            fee_bps: 30,
        };

        let bids = OrderBookSide {
            orders: Map::new(&env),
            price_levels: Map::new(&env),
        };

        let asks = OrderBookSide {
            orders: Map::new(&env),
            price_levels: Map::new(&env),
        };

        env.storage().instance().set(&DataKey::Config, &config);
        env.storage().persistent().set(&DataKey::Bids, &bids);
        env.storage().persistent().set(&DataKey::Asks, &asks);

        env.events().publish(
            (Symbol::new(&env, "init"),),
            (admin, base_token, quote_token),
        );
    }

    pub fn place_order(
        env: Env,
        trader: Address,
        order_type: u32,
        price: u64,
        amount: u128,
    ) -> u64 {
        trader.require_auth();

        let config = get_config(&env);
        let order_type = match order_type {
            0 => OrderType::Bid,
            1 => OrderType::Ask,
            _ => panic!("Invalid order type"),
        };

        if price == 0 {
            panic!("Invalid price");
        }
        if amount == 0 {
            panic!("Invalid amount");
        }

        let order_id = config.next_order_id;
        let mut new_config = config.clone();
        new_config.next_order_id = order_id.saturating_add(1);
        env.storage().instance().set(&DataKey::Config, &new_config);

        let order = Order {
            id: order_id,
            trader: trader.clone(),
            order_type,
            price,
            amount,
        };

        let (filled_amount, remaining_amount) = match_orders(&env, &mut new_config, &order);

        if remaining_amount > 0 {
            let mut remaining_order = order.clone();
            remaining_order.amount = remaining_amount;
            insert_order(&env, &remaining_order);
        }

        env.events().publish(
            (Symbol::new(&env, "order"), order_id),
            (trader, order_type as u32, price, amount, filled_amount),
        );

        order_id
    }

    pub fn cancel_order(env: Env, trader: Address, order_id: u64) {
        trader.require_auth();

        let order_key = DataKey::Order(order_id);
        let order: Order = env.storage().persistent()
            .get(&order_key)
            .unwrap_or_else(|| panic!("Order not found"));

        if order.trader != trader {
            panic!("Order not owned by trader");
        }

        remove_order(&env, &order);
        env.storage().persistent().remove(&order_key);

        env.events().publish(
            (Symbol::new(&env, "cancel"), order_id),
            trader,
        );
    }

    pub fn get_order(env: Env, order_id: u64) -> Option<Order> {
        let order_key = DataKey::Order(order_id);
        env.storage().persistent().get(&order_key)
    }

    pub fn get_config(env: Env) -> Config {
        env.storage().instance()
            .get(&DataKey::Config)
            .unwrap_or_else(|| panic!("Contract not initialized"))
    }

    pub fn get_bids(env: Env) -> OrderBookSide {
        env.storage().persistent()
            .get(&DataKey::Bids)
            .unwrap_or_else(|| OrderBookSide {
                orders: Map::new(&env),
                price_levels: Map::new(&env),
            })
    }

    pub fn get_asks(env: Env) -> OrderBookSide {
        env.storage().persistent()
            .get(&DataKey::Asks)
            .unwrap_or_else(|| OrderBookSide {
                orders: Map::new(&env),
                price_levels: Map::new(&env),
            })
    }

    pub fn set_fee(env: Env, admin: Address, fee_bps: u32) {
        admin.require_auth();
        let mut config = get_config(&env);
        if config.admin != admin {
            panic!("Unauthorized");
        }
        config.fee_bps = fee_bps;
        env.storage().instance().set(&DataKey::Config, &config);
    }
}

fn get_config(env: &Env) -> Config {
    env.storage().instance()
        .get(&DataKey::Config)
        .unwrap_or_else(|| panic!("Contract not initialized"))
}

fn insert_order(env: &Env, order: &Order) {
    let (bids_key, asks_key) = (DataKey::Bids, DataKey::Asks);
    let side_key = match order.order_type {
        OrderType::Bid => bids_key,
        OrderType::Ask => asks_key,
    };

    let mut side: OrderBookSide = env.storage().persistent()
        .get(&side_key)
        .unwrap_or_else(|| OrderBookSide {
            orders: Map::new(env),
            price_levels: Map::new(env),
        });

    side.orders.set(order.id, order.clone());

    let mut price_orders: Vec<u64> = side.price_levels
        .get(order.price)
        .unwrap_or_else(|| Vec::new(env));
    price_orders.push_back(order.id);
    side.price_levels.set(order.price, price_orders);

    env.storage().persistent().set(&side_key, &side);
    env.storage().persistent().set(&DataKey::Order(order.id), order);
}

fn remove_order(env: &Env, order: &Order) {
    let (bids_key, asks_key) = (DataKey::Bids, DataKey::Asks);
    let side_key = match order.order_type {
        OrderType::Bid => bids_key,
        OrderType::Ask => asks_key,
    };

    let mut side: OrderBookSide = env.storage().persistent()
        .get(&side_key)
        .unwrap_or_else(|| OrderBookSide {
            orders: Map::new(env),
            price_levels: Map::new(env),
        });

    side.orders.remove(order.id);

    if let Some(price_orders) = side.price_levels.get(order.price) {
        let mut filtered: Vec<u64> = Vec::new(env);
        for id in price_orders.iter() {
            if id != order.id {
                filtered.push_back(id);
            }
        }
        if filtered.is_empty() {
            side.price_levels.remove(order.price);
        } else {
            side.price_levels.set(order.price, filtered);
        }
    }

    env.storage().persistent().set(&side_key, &side);
}
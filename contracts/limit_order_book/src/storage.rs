use soroban_sdk::{Env, Map, Vec};

use crate::{Config, DataKey, Order, OrderBookSide, OrderType, OrderBookError};

pub fn get_config(env: &Env) -> Config {
    env.storage().instance()
        .get(&DataKey::Config)
        .unwrap_or_else(|| panic!("Contract not initialized"))
}

pub fn get_bids(env: &Env) -> OrderBookSide {
    env.storage().persistent()
        .get(&DataKey::Bids)
        .unwrap_or_else(|| OrderBookSide {
            orders: Map::new(env),
            price_levels: Map::new(env),
        })
}

pub fn get_asks(env: &Env) -> OrderBookSide {
    env.storage().persistent()
        .get(&DataKey::Asks)
        .unwrap_or_else(|| OrderBookSide {
            orders: Map::new(env),
            price_levels: Map::new(env),
        })
}

pub fn get_order(env: &Env, order_id: u64) -> Option<Order> {
    env.storage().persistent().get(&DataKey::Order(order_id))
}

pub fn set_config(env: &Env, config: &Config) {
    env.storage().instance().set(&DataKey::Config, config);
}

pub fn set_bids(env: &Env, bids: &OrderBookSide) {
    env.storage().persistent().set(&DataKey::Bids, bids);
}

pub fn set_asks(env: &Env, asks: &OrderBookSide) {
    env.storage().persistent().set(&DataKey::Asks, asks);
}

pub fn set_order(env: &Env, order: &Order) {
    env.storage().persistent().set(&DataKey::Order(order.id), order);
}

pub fn remove_order_storage(env: &Env, order_id: u64) {
    env.storage().persistent().remove(&DataKey::Order(order_id));
}

pub fn get_next_order_id(env: &Env) -> u64 {
    let config = get_config(env);
    config.next_order_id
}

pub fn increment_order_id(env: &Env) {
    let mut config = get_config(env);
    config.next_order_id = config.next_order_id.saturating_add(1);
    set_config(env, &config);
}

pub fn get_orders_at_price(env: &Env, order_type: OrderType, price: u64) -> Vec<u64> {
    let side = match order_type {
        OrderType::Bid => get_bids(env),
        OrderType::Ask => get_asks(env),
    };
    
    side.price_levels.get(price).unwrap_or_else(|| Vec::new(env))
}

pub fn get_order_count(env: &Env, order_type: OrderType) -> u32 {
    let side = match order_type {
        OrderType::Bid => get_bids(env),
        OrderType::Ask => get_asks(env),
    };
    
    side.orders.len()
}

pub fn get_total_volume(env: &Env, order_type: OrderType) -> u128 {
    let side = match order_type {
        OrderType::Bid => get_bids(env),
        OrderType::Ask => get_asks(env),
    };
    
    let mut total: u128 = 0;
    for order in side.orders.values() {
        total = total.saturating_add(order.amount);
    }
    total
}
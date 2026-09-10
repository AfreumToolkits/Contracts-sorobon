use soroban_sdk::{Env, Map, Vec, Symbol, token};

use crate::{Config, DataKey, Order, OrderBookSide, OrderType};

pub fn match_orders(
    env: &Env,
    config: &mut Config,
    incoming_order: &Order,
) -> (u128, u128) {
    let opposite_key = match incoming_order.order_type {
        OrderType::Bid => DataKey::Asks,
        OrderType::Ask => DataKey::Bids,
    };

    let mut opposite_side: OrderBookSide = env.storage().persistent()
        .get(&opposite_key)
        .unwrap_or_else(|| OrderBookSide {
            orders: Map::new(env),
            price_levels: Map::new(env),
        });

    let mut remaining = incoming_order.amount;
    let mut filled = 0u128;

    let price_levels = get_matching_prices(env, &opposite_side, incoming_order);

    for price in price_levels.iter() {
        if remaining == 0 {
            break;
        }

        if let Some(mut order_ids) = opposite_side.price_levels.get(price) {
            let mut i = 0;
            while i < order_ids.len() && remaining > 0 {
                let maker_order_id = order_ids.get(i).unwrap();
                
                if let Some(mut maker_order) = opposite_side.orders.get(maker_order_id) {
                    if can_match(incoming_order, &maker_order, price) {
                        let trade_amount = remaining.min(maker_order.amount);
                        
                        execute_trade(
                            env,
                            config,
                            incoming_order,
                            &maker_order,
                            trade_amount,
                            price,
                        );
                        
                        remaining = remaining.saturating_sub(trade_amount);
                        filled = filled.saturating_add(trade_amount);
                        maker_order.amount = maker_order.amount.saturating_sub(trade_amount);

                        if maker_order.amount == 0 {
                            opposite_side.orders.remove(maker_order_id);
                            let mut new_ids: Vec<u64> = Vec::new(env);
                            for j in 0..order_ids.len() {
                                if j != i {
                                    new_ids.push_back(order_ids.get(j).unwrap());
                                }
                            }
                            order_ids = new_ids;
                        } else {
                            opposite_side.orders.set(maker_order_id, maker_order.clone());
                            i += 1;
                        }
                    } else {
                        i += 1;
                    }
                } else {
                    let mut new_ids: Vec<u64> = Vec::new(env);
                    for j in 0..order_ids.len() {
                        if j != i {
                            new_ids.push_back(order_ids.get(j).unwrap());
                        }
                    }
                    order_ids = new_ids;
                }
            }

            if order_ids.is_empty() {
                opposite_side.price_levels.remove(price);
            } else {
                opposite_side.price_levels.set(price, order_ids);
            }
        }
    }

    env.storage().persistent().set(&opposite_key, &opposite_side);
    (filled, remaining)
}

fn get_matching_prices(
    env: &Env,
    opposite_side: &OrderBookSide,
    incoming_order: &Order,
) -> Vec<u64> {
    let mut prices: Vec<u64> = Vec::new(env);
    
    for price in opposite_side.price_levels.keys() {
        let should_match = match incoming_order.order_type {
            OrderType::Bid => price <= incoming_order.price,
            OrderType::Ask => price >= incoming_order.price,
        };
        
        if should_match {
            prices.push_back(price);
        }
    }

    let mut sorted: Vec<u64> = Vec::new(env);
    let mut price_vec: Vec<u64> = Vec::new(env);
    for p in prices.iter() {
        price_vec.push_back(p);
    }
    
    match incoming_order.order_type {
        OrderType::Bid => {
            sort_desc(&mut price_vec);
        }
        OrderType::Ask => {
            sort_asc(&mut price_vec);
        }
    }
    
    for p in price_vec.iter() {
        sorted.push_back(p);
    }
    
    sorted
}

fn sort_asc(vec: &mut Vec<u64>) {
    let len = vec.len();
    for i in 0..len {
        for j in 0..len - 1 - i {
            let a = vec.get(j).unwrap();
            let b = vec.get(j + 1).unwrap();
            if a > b {
                vec.set(j, b);
                vec.set(j + 1, a);
            }
        }
    }
}

fn sort_desc(vec: &mut Vec<u64>) {
    let len = vec.len();
    for i in 0..len {
        for j in 0..len - 1 - i {
            let a = vec.get(j).unwrap();
            let b = vec.get(j + 1).unwrap();
            if a < b {
                vec.set(j, b);
                vec.set(j + 1, a);
            }
        }
    }
}

fn can_match(_incoming: &Order, _maker: &Order, maker_price: u64) -> bool {
    true
}

fn execute_trade(
    env: &Env,
    config: &Config,
    taker_order: &Order,
    maker_order: &Order,
    amount: u128,
    price: u64,
) {
    let base_client = token::Client::new(env, &config.base_token);
    let quote_client = token::Client::new(env, &config.quote_token);

    let (taker, maker) = match taker_order.order_type {
        OrderType::Bid => (taker_order.trader.clone(), maker_order.trader.clone()),
        OrderType::Ask => (maker_order.trader.clone(), taker_order.trader.clone()),
    };

    let base_amount = amount as i128;
    let quote_amount = calculate_quote_amount(amount, price, config.fee_bps) as i128;

    base_client.transfer(&taker, &maker, &base_amount);
    quote_client.transfer(&maker, &taker, &quote_amount);

    env.events().publish(
        (Symbol::new(env, "trade"),),
        (taker_order.id, maker_order.id, price, amount, quote_amount),
    );
}

fn calculate_quote_amount(base_amount: u128, price: u64, fee_bps: u32) -> u128 {
    let gross = (base_amount as u128)
        .saturating_mul(price as u128)
        .saturating_div(10_000_000);
    
    let fee = gross.saturating_mul(fee_bps as u128).saturating_div(10_000);
    gross.saturating_sub(fee)
}

pub fn get_best_bid(env: &Env) -> Option<u64> {
    let bids: OrderBookSide = env.storage().persistent()
        .get(&DataKey::Bids)
        .unwrap_or_else(|| OrderBookSide {
            orders: Map::new(env),
            price_levels: Map::new(env),
        });
    
    let mut max_price: Option<u64> = None;
    for price in bids.price_levels.keys() {
        match max_price {
            None => max_price = Some(price),
            Some(max) if price > max => max_price = Some(price),
            _ => {}
        }
    }
    max_price
}

pub fn get_best_ask(env: &Env) -> Option<u64> {
    let asks: OrderBookSide = env.storage().persistent()
        .get(&DataKey::Asks)
        .unwrap_or_else(|| OrderBookSide {
            orders: Map::new(env),
            price_levels: Map::new(env),
        });
    
    let mut min_price: Option<u64> = None;
    for price in asks.price_levels.keys() {
        match min_price {
            None => min_price = Some(price),
            Some(min) if price < min => min_price = Some(price),
            _ => {}
        }
    }
    min_price
}

pub fn get_spread(env: &Env) -> Option<(u64, u64)> {
    match (get_best_bid(env), get_best_ask(env)) {
        (Some(bid), Some(ask)) => Some((bid, ask)),
        _ => None,
    }
}
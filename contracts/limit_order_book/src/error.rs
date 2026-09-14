use soroban_sdk::{contracterror, Env};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OrderBookError {
    NotInitialized = 1,
    Unauthorized = 2,
    InvalidOrderType = 3,
    InsufficientBalance = 4,
    OrderNotFound = 5,
    InvalidPrice = 6,
    InvalidAmount = 7,
    OrderNotOwned = 8,
    TransferFailed = 9,
}

impl OrderBookError {
    pub fn to_string(&self, env: &Env) -> soroban_sdk::String {
        match self {
            OrderBookError::NotInitialized => soroban_sdk::String::from_str(env, "Contract not initialized"),
            OrderBookError::Unauthorized => soroban_sdk::String::from_str(env, "Unauthorized"),
            OrderBookError::InvalidOrderType => soroban_sdk::String::from_str(env, "Invalid order type"),
            OrderBookError::InsufficientBalance => soroban_sdk::String::from_str(env, "Insufficient balance"),
            OrderBookError::OrderNotFound => soroban_sdk::String::from_str(env, "Order not found"),
            OrderBookError::InvalidPrice => soroban_sdk::String::from_str(env, "Invalid price"),
            OrderBookError::InvalidAmount => soroban_sdk::String::from_str(env, "Invalid amount"),
            OrderBookError::OrderNotOwned => soroban_sdk::String::from_str(env, "Order not owned by trader"),
            OrderBookError::TransferFailed => soroban_sdk::String::from_str(env, "Token transfer failed"),
        }
    }
}
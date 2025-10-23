use alloc::vec::Vec;
use alloy_primitives::{Address, Bytes, B256, U256};
use reth_revm::{db::Cache, state::Bytecode};

pub struct FlatExecutionWitness {
    // pub accounts: Vec<Account>,
    // pub contracts: Vec<Bytecode>,
    pub cache: Cache,
    pub headers: Vec<Bytes>,
}

pub struct Account {
    /// Account address
    address: Address,
    /// Account balance.
    pub balance: U256,
    /// Account nonce.
    pub nonce: u64,
    /// Hash of the raw bytes in `code`, or [`KECCAK_EMPTY`].
    pub code_hash: B256,
}

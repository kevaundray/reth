use alloc::vec::Vec;
use alloy_primitives::{Address, Bytes, B256, U256};
use reth_revm::{db::Cache, state::Bytecode};

#[derive(Debug, Clone)]
pub struct FlatExecutionWitness {
    // pub accounts: Vec<Account>,
    // pub contracts: Vec<Bytecode>,
    pub cache: Cache,
    pub headers: Vec<Bytes>,
}

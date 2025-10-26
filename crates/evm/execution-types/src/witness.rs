//! Witness recording types for EVM execution.

use alloy_primitives::{keccak256, Address, B256, U256};
use revm::{
    database::{AccountStatus, DbAccount, State},
    primitives::{HashMap, HashSet},
    state::Bytecode,
};

/// Records pre-state data for witness generation.
#[derive(Debug, Clone, Default)]
pub struct FlatPreState {
    /// Accounts accessed during execution.
    pub accounts: HashMap<Address, DbAccount>,
    /// Bytecode accessed during execution.
    pub contracts: HashMap<B256, Bytecode>,
    /// The set of addresses that have been self-destructed in the execution.
    pub destructed_addresses: HashSet<Address>,
}

/// Records pre-state accesses that occurred during execution.
#[derive(Debug, Clone, Default)]
pub struct FlatWitnessRecord {
    /// Accounts accessed during execution.
    pub accounts: HashMap<Address, AccessedAccount>,
    /// Bytecode accessed during execution.
    pub contracts: HashMap<B256, Option<Bytecode>>,
}

/// Represents an accessed account during execution.
#[derive(Debug, Clone)]
pub enum AccessedAccount {
    /// Indicates if the account was destroyed during execution.
    Destroyed,
    /// Storage keys accessed during execution.
    StorageKeys(HashSet<U256>),
}

impl FlatWitnessRecord {
    /// Records the accessed state after execution.
    pub fn record_executed_state<DB>(&mut self, statedb: &State<DB>) {
        self.contracts = statedb
            .cache
            .contracts
            .values()
            .map(|code| code.original_bytes())
            .map(|code_bytes| (keccak256(&code_bytes), Some(Bytecode::new_legacy(code_bytes))))
            .collect();

        for (address, account) in &statedb.cache.accounts {
            let account = match account.status {
                AccountStatus::Destroyed => AccessedAccount::Destroyed,
                _ => {
                    let storage_keys = account
                        .account
                        .as_ref()
                        .map_or_else(HashSet::default, |a| a.storage.keys().copied().collect());
                    AccessedAccount::StorageKeys(storage_keys)
                }
            };
            self.accounts.insert(*address, account);
        }
    }
}

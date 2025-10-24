//! Flat execution witness for stateless block validation.
//!
//! This module provides a simplified witness structure containing the minimal state
//! data required for stateless execution of a block. The "flat" representation stores
//! state directly in a cache rather than as a Merkle proof, optimizing for execution
//! speed. This is useful if the flat execution witness is later cryptographically proven
//! correct in an independent proof.

use core::error;

use alloc::fmt;
use alloy_primitives::{map::HashMap, Address, StorageValue, B256, U256};
use reth_primitives_traits::Header;
use reth_revm::{
    db::{Cache, CacheDB, DBErrorMarker, DbAccount},
    primitives::StorageKey,
    state::{AccountInfo, Bytecode},
    Database,
};

/// A flat execution witness containing the state and context needed for stateless block execution.
#[derive(Debug, Clone)]
pub struct FlatExecutionWitness {
    /// The state required for executing the block.
    pub state: Cache,
    /// The parent block header required for pre-execution validations.
    pub parent_header: Header,
}

impl FlatExecutionWitness {
    /// Creates a new flat execution witness from state components.
    pub fn new(
        accounts: HashMap<Address, DbAccount>,
        contracts: HashMap<B256, Bytecode>,
        block_hashes: HashMap<U256, B256>,
        parent_header: Header,
    ) -> Self {
        Self {
            state: Cache { accounts, contracts, block_hashes, ..Default::default() },
            parent_header,
        }
    }

    /// Creates a cached database from the witness state.
    ///
    /// Returns a `CacheDB` backed by `FailingDB`, which ensures all state must come from the
    /// cache. Any cache miss results in an error, enforcing stateless execution constraints.
    pub fn create_db(&self) -> CacheDB<FailingDB> {
        CacheDB { cache: self.state.clone(), db: FailingDB }
    }
}

/// Error type that always indicates database operation failure.
///
/// Used by `FailingDB` to signal that state access attempted to reach beyond the witness cache.
/// In stateless execution, all required state must be present in the witness - cache misses
/// indicate incomplete witness data. This is critical since if we don't do this the execution
/// could access state that isn't cryptographically verified against the trie witness.
#[derive(Debug)]
pub struct AlwaysFailError;

impl fmt::Display for AlwaysFailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Database access not allowed in stateless execution context")
    }
}

impl error::Error for AlwaysFailError {}
impl DBErrorMarker for AlwaysFailError {}

/// Database implementation that fails all operations.
#[derive(Debug)]
pub struct FailingDB;

impl Database for FailingDB {
    type Error = AlwaysFailError;

    fn basic(&mut self, _address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        Err(AlwaysFailError)
    }

    fn code_by_hash(&mut self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        Err(AlwaysFailError)
    }

    fn storage(
        &mut self,
        _address: Address,
        _index: StorageKey,
    ) -> Result<StorageValue, Self::Error> {
        Err(AlwaysFailError)
    }

    fn block_hash(&mut self, _number: u64) -> Result<B256, Self::Error> {
        Err(AlwaysFailError)
    }
}

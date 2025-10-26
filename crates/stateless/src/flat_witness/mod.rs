//! Flat execution witness for stateless block validation.
//!
//! This module provides a simplified witness structure containing the minimal state
//! data required for stateless execution of a block. The "flat" representation stores
//! state directly in a cache rather than as a Merkle proof, optimizing for execution
//! speed. This is useful if the flat execution witness is later cryptographically proven
//! correct in an independent proof.

use core::error;

pub mod bincode;

use alloc::fmt;
use alloy_primitives::{
    map::{HashMap, HashSet},
    Address, StorageValue, B256, U256,
};
use reth_primitives_traits::Header;
use reth_revm::{
    db::{Cache, CacheDB, DBErrorMarker, DbAccount},
    primitives::StorageKey,
    state::{AccountInfo, Bytecode},
    DatabaseRef,
};
use serde_with::serde_as;

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

/// A flat execution witness containing the state and context needed for stateless block execution.
#[serde_with::serde_as]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FlatExecutionWitness {
    /// The state required for executing the block.
    #[serde_as(as = "bincode::CacheBincode")]
    pub state: Cache,
    /// The parent block header required for pre-execution validations.
    pub parent_header: Header,
    /// The set of addresses that have been self-destructed in the execution.
    pub destructed_addresses: HashSet<Address>,
}

impl FlatExecutionWitness {
    /// Creates a new flat execution witness from state components.
    pub fn new(
        pre_state: FlatPreState,
        block_hashes: HashMap<U256, B256>,
        parent_header: Header,
    ) -> Self {
        Self {
            state: Cache {
                accounts: pre_state.accounts,
                contracts: pre_state.contracts,
                block_hashes,
                logs: Default::default(),
            },
            destructed_addresses: pre_state.destructed_addresses,
            parent_header,
        }
    }

    /// Creates a cached database from the witness state.
    ///
    /// Returns a `CacheDB` backed by `FailingDB`, which ensures all state must come from the
    /// cache. Any cache miss results in an error, enforcing stateless execution constraints.
    // pub fn create_db(self) -> CacheDB<reth_revm::db::EmptyDB> {
    pub fn create_db(self) -> CacheDB<SelfDestructCompatibleFailingDB> {
        CacheDB {
            cache: self.state,
            db: SelfDestructCompatibleFailingDB::new(self.destructed_addresses),
        }
    }
}

/// Database backend that fails on all accesses except storage reads from self-destructed accounts.
///
/// This enforces that all state accesses during execution must be present in the cache. Cache
/// misses indicate missing witness data and must fail, since only cached accesses are
/// cryptographically verified against the trie witness proof. Returning default values would bypass
/// verification.
///
/// **Exception: Self-destructed account storage**
///
/// Storage reads from self-destructed accounts return zero (`Default::default()`) on cache miss.
/// This is safe because the trie witness includes complete storage tries for self-destructed
/// accounts, not just accessed slots—allowing any non-existent slot to be proven as zero. This
/// behavior matches the witness generation in `TrieWitness::get_proof_targets` and compensates for
/// `StateDB` not tracking individual storage accesses of self-destructed accounts.
#[derive(Debug, Clone)]
pub struct SelfDestructCompatibleFailingDB {
    destructed_addresses: HashSet<Address>,
}

impl SelfDestructCompatibleFailingDB {
    /// Creates a new instance with the given set of self-destructed addresses.
    pub const fn new(destructed_addresses: HashSet<Address>) -> Self {
        Self { destructed_addresses }
    }
}

/// Error indicating that a database access was attempted outside the captured state.
#[derive(Debug)]
pub struct NonCapturedStateError;

impl fmt::Display for NonCapturedStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Database access not allowed in stateless execution context")
    }
}

impl error::Error for NonCapturedStateError {}
impl DBErrorMarker for NonCapturedStateError {}

impl DatabaseRef for SelfDestructCompatibleFailingDB {
    type Error = NonCapturedStateError;

    fn basic_ref(&self, _address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        Err(NonCapturedStateError)
    }

    fn code_by_hash_ref(&self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        Err(NonCapturedStateError)
    }

    fn storage_ref(
        &self,
        _address: Address,
        _index: StorageKey,
    ) -> Result<StorageValue, Self::Error> {
        if self.destructed_addresses.contains(&_address) {
            return Ok(Default::default());
        }
        Err(NonCapturedStateError)
    }

    fn block_hash_ref(&self, _number: u64) -> Result<B256, Self::Error> {
        Err(NonCapturedStateError)
    }
}

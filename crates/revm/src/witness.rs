use alloc::vec::Vec;
use alloy_primitives::{keccak256, Address, Bytes, B256, U256};
use reth_db::{
    cursor::{DbCursorRO, DbDupCursorRO},
    tables,
    transaction::DbTx,
};
use reth_provider::{providers::NodeTypesForProvider, DatabaseProvider};
use reth_trie::{HashedPostState, HashedStorage};
use revm::{
    database::{AccountStatus, DbAccount, State},
    primitives::{HashMap, HashSet},
    state::Bytecode,
};

/// Tracks state changes during execution.
#[derive(Debug, Clone, Default)]
pub struct ExecutionWitnessRecord {
    /// Records all state changes
    pub hashed_state: HashedPostState,
    /// Map of all contract codes (created / accessed) to their preimages that were required during
    /// the execution of the block, including during state root recomputation.
    ///
    /// `keccak(bytecodes) => bytecodes`
    pub codes: Vec<Bytes>,
    /// Map of all hashed account and storage keys (addresses and slots) to their preimages
    /// (unhashed account addresses and storage slots, respectively) that were required during
    /// the execution of the block.
    ///
    /// `keccak(address|slot) => address|slot`
    pub keys: Vec<Bytes>,
    /// The lowest block number referenced by any BLOCKHASH opcode call during transaction
    /// execution.
    ///
    /// This helps determine which ancestor block headers must be included in the
    /// `ExecutionWitness`.
    ///
    /// `None` - when the BLOCKHASH opcode was not called during execution
    pub lowest_block_number: Option<u64>,
}

impl ExecutionWitnessRecord {
    /// Records the state after execution.
    pub fn record_executed_state<DB>(&mut self, statedb: &State<DB>) {
        self.codes = statedb
            .cache
            .contracts
            .values()
            .map(|code| code.original_bytes())
            .chain(
                // cache state does not have all the contracts, especially when
                // a contract is created within the block
                // the contract only exists in bundle state, therefore we need
                // to include them as well
                statedb.bundle_state.contracts.values().map(|code| code.original_bytes()),
            )
            .collect();

        for (address, account) in &statedb.cache.accounts {
            let hashed_address = keccak256(address);
            self.hashed_state
                .accounts
                .insert(hashed_address, account.account.as_ref().map(|a| (&a.info).into()));

            let storage = self
                .hashed_state
                .storages
                .entry(hashed_address)
                .or_insert_with(|| HashedStorage::new(account.status.was_destroyed()));

            if let Some(account) = &account.account {
                self.keys.push(address.to_vec().into());

                for (slot, value) in &account.storage {
                    let slot = B256::from(*slot);
                    let hashed_slot = keccak256(slot);
                    storage.storage.insert(hashed_slot, *value);

                    self.keys.push(slot.into());
                }
            }
        }
        // BTreeMap keys are ordered, so the first key is the smallest
        self.lowest_block_number = statedb.block_hashes.keys().next().copied()
    }

    /// Creates the record from the state after execution.
    pub fn from_executed_state<DB>(state: &State<DB>) -> Self {
        let mut record = Self::default();
        record.record_executed_state(state);
        record
    }
}

/// Errors that can occur during flat witness record generation.
#[derive(Debug, thiserror::Error)]
pub enum FlatWitnessRecordError {
    /// Failed to retrieve account from state provider
    #[error("Failed to get prestate account: {0}")]
    GetAccountError(#[from] reth_provider::ProviderError),
    /// Failed to access database cursor
    #[error("Failed to create database cursor: {0}")]
    DatabaseCursorError(#[from] reth_db::DatabaseError),
}

/// Records pre-state data for witness generation.
#[derive(Debug, Clone, Default)]
pub struct FlatWitnessRecord {
    /// Accounts accessed during execution.
    pub accounts: HashMap<Address, DbAccount>,
    /// Bytecode accessed during execution.
    pub contracts: HashMap<B256, Bytecode>,
    /// Ancestor block hashes required by BLOCKHASH.
    pub block_hashes: HashMap<U256, B256>,
    /// The set of addresses that have been self-destructed in the execution.
    pub destructed_addresses: HashSet<Address>,
}

impl FlatWitnessRecord {
    /// Records pre-state from database for all accounts touched during execution.
    pub fn record_executed_state<DB, TX: DbTx + 'static, N: NodeTypesForProvider>(
        &mut self,
        statedb: &State<DB>,
        provider: &DatabaseProvider<TX, N>,
    ) -> Result<(), FlatWitnessRecordError> {
        // The provided statedb contains post-execution state, so we use the provider to pull the
        // pre-state of the accessed accounts, storage slots, and codes.
        let state_provider = provider.latest();

        self.contracts = statedb
            .cache
            .contracts
            .values()
            .map(|code| code.original_bytes())
            .map(|code_bytes| (keccak256(&code_bytes), Bytecode::new_legacy(code_bytes)))
            .collect();

        for (address, account) in &statedb.cache.accounts {
            let provider_account = state_provider.basic_account(address)?;

            let mut db_account = match provider_account {
                Some(account) => DbAccount { info: account.into(), ..Default::default() },
                None => DbAccount::new_not_existing(),
            };

            // Fetch pre-state storage for accessed slots
            if let Some(account) = &account.account {
                for slot in account.storage.keys() {
                    let val = state_provider.storage(*address, (*slot).into())?.unwrap_or_default();
                    db_account.storage.insert(*slot, val);
                }
            }

            // When an account is destroyed, statedb discards all storage slots, losing track of
            // which slots were accessed pre-destruction. We fetch all storage slots for destroyed
            // accounts (mirroring TrieWitness::get_proof_targets in trie/trie/src/witness.rs),
            // requiring a lower-level db cursor. Only relevant pre-Cancun where SELFDESTRUCT clears
            // storage.
            if account.status == AccountStatus::Destroyed {
                let tx = provider.tx_ref();
                let mut storage_cursor = tx.cursor_dup_read::<tables::PlainStorageState>()?;
                if let Some((_, first_entry)) = storage_cursor.seek_exact(*address)? {
                    db_account
                        .storage
                        .insert(U256::from_be_bytes(first_entry.key.0), first_entry.value);

                    while let Some((_, entry)) = storage_cursor.next_dup()? {
                        db_account.storage.insert(U256::from_be_bytes(entry.key.0), entry.value);
                    }
                }
                self.destructed_addresses.insert(*address);
            }

            self.accounts.insert(*address, db_account);
        }

        Ok(())
    }
}

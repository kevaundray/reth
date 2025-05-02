use alloy_primitives::{keccak256, map::B256Map, Address, Bytes, B256, U256};
use reth_revm::{
    state::{AccountInfo, Bytecode},
    witness::ExecutionWitnessRecord,
    Database,
};
use reth_trie::{HashedPostState, HashedStorage};

use alloy_primitives::map::HashSet;
/// The state witness recorder that records all state accesses during execution.
/// It does so by implementing the [`reth_revm::Database`] and recording accesses of accounts and
/// slots.
pub(crate) struct StateWitnessRecorderDatabase<D> {
    database: D,
    // The following fields are essentially the ExecutionWitnessRecord without the preimages.
    // We are extending the strategy that ress uses so that we get witnesses for
    // invalid blocks.
    state: HashedPostState,
    codes: B256Map<Bytes>,
    lowest_block_number: Option<u64>,
}

impl<D> StateWitnessRecorderDatabase<D> {
    pub(crate) fn new(database: D) -> Self {
        Self {
            database,
            state: Default::default(),
            codes: Default::default(),
            lowest_block_number: None,
        }
    }

    pub(crate) fn execution_witness_record(self) -> ExecutionWitnessRecord {
        ExecutionWitnessRecord {
            hashed_state: self.state,
            codes: self.codes.values().cloned().collect(),
            keys: Vec::new(),
            lowest_block_number: self.lowest_block_number,
        }
    }
}

impl<D: Database> Database for StateWitnessRecorderDatabase<D> {
    type Error = D::Error;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        let maybe_account = self.database.basic(address)?;
        let hashed_address = keccak256(address);
        tracing::info!(address = %address, hashed_address = %hashed_address);
        self.state.accounts.insert(hashed_address, maybe_account.as_ref().map(|acc| acc.into()));
        Ok(maybe_account)
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        let value = self.database.storage(address, index)?;
        let hashed_address = keccak256(address);
        let hashed_slot = keccak256(B256::from(index));
        self.state
            .storages
            .entry(hashed_address)
            .or_insert_with(|| HashedStorage::new(false))
            .storage
            .insert(hashed_slot, value);
        Ok(value)
    }

    // TODO: Something to note, for invalid blocks, it might be a dos vector
    // TODO: If the block is invalid because it tried to access, lets say block 0
    // TODO: via the BLOCKHASH opcode. If the code adds all headers from current to
    // TODO: 0, then that will likely not be provable.
    // TODO: We should just keep the most recent 256
    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        let block_hash = self.database.block_hash(number)?;
        if let Some(lowest) = self.lowest_block_number {
            self.lowest_block_number = Some(lowest.min(number));
        } else {
            self.lowest_block_number = Some(number);
        }
        Ok(block_hash)
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        let bytecode = self.database.code_by_hash(code_hash)?;
        self.codes.insert(code_hash, bytecode.bytes());
        Ok(bytecode)
    }
}

// TODO: Possibly put as a method on `ExecutionWitnessRecord`
// TODO: Leaving it here to keep the changes in the ress directory
pub(crate) fn merge_execution_witness_records(
    lhs: ExecutionWitnessRecord,
    rhs: ExecutionWitnessRecord,
) -> ExecutionWitnessRecord {
    // Merge the two execution witness records
    //
    // Merge the hashed post state
    let mut hashed_state = lhs.hashed_state;
    hashed_state.extend(rhs.hashed_state);
    //
    // Merge bytecode
    let codes: Vec<_> = {
        let mut merged: HashSet<_> = lhs.codes.into_iter().collect();
        merged.extend(rhs.codes);
        merged.into_iter().collect()
    };
    // Merge lowest accessed block number
    let lowest_block_number = {
        let a = lhs.lowest_block_number;
        let b = rhs.lowest_block_number;
        match (a, b) {
            (Some(a_val), Some(b_val)) => Some(a_val.min(b_val)),
            // Since we know that they are either both `None` or one of them is `None`
            // `or` will return the `Some` value or `None` if they are both `None`
            _ => a.or(b),
        }
    };

    // Note: We do not merge the preimages because this is not useful for us
    ExecutionWitnessRecord { hashed_state, codes, keys: Vec::new(), lowest_block_number }
}
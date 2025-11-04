#![allow(missing_docs)]

use alloc::vec::Vec;
use alloy_primitives::{Address, Bytes, B256, U256};
use reth_primitives_traits::Account;
use reth_revm::{
    db::{AccountState, Cache, DbAccount},
    primitives::{StorageKey, StorageValue},
    state::{AccountInfo, Bytecode},
};
use reth_trie_common::{HashedPostState, HashedStorage};
use serde_with::{DeserializeAs, SerializeAs};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheBincode {
    pub accounts: Vec<(Address, DbAccountBincode)>,
    pub contracts: Vec<(B256, Bytes)>,
    pub block_hashes: Vec<(U256, B256)>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DbAccountBincode {
    pub info: AccountInfo,
    pub account_state: AccountState,
    pub storage: Vec<(StorageKey, StorageValue)>,
}

impl From<DbAccount> for DbAccountBincode {
    fn from(account: DbAccount) -> Self {
        let mut storage: Vec<_> = account.storage.into_iter().collect();
        storage.sort_unstable_by_key(|(k, _)| *k);
        Self { info: account.info, account_state: account.account_state, storage }
    }
}

impl From<DbAccountBincode> for DbAccount {
    fn from(val: DbAccountBincode) -> Self {
        Self {
            info: val.info,
            account_state: val.account_state,
            storage: val.storage.into_iter().collect(),
        }
    }
}

impl From<&Cache> for CacheBincode {
    fn from(cache: &Cache) -> Self {
        let mut accounts: Vec<_> =
            cache.accounts.iter().map(|(k, v)| (*k, v.clone().into())).collect();
        accounts.sort_unstable_by_key(|(k, _)| *k);

        let mut contracts: Vec<_> =
            cache.contracts.iter().map(|(k, v)| (*k, v.original_bytes())).collect();
        contracts.sort_unstable_by_key(|(k, _)| *k);

        let mut block_hashes: Vec<_> = cache.block_hashes.iter().map(|(k, v)| (*k, *v)).collect();
        block_hashes.sort_unstable_by_key(|(k, _)| *k);

        Self { accounts, contracts, block_hashes }
    }
}

impl SerializeAs<Cache> for CacheBincode {
    fn serialize_as<S>(source: &Cache, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::Serialize;
        let cache_bincode: Self = source.into();
        cache_bincode.serialize(serializer)
    }
}

impl<'de> DeserializeAs<'de, Cache> for CacheBincode {
    fn deserialize_as<D>(deserializer: D) -> Result<Cache, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::Deserialize;
        let cache_bincode = Self::deserialize(deserializer)?;
        Ok(Cache {
            accounts: cache_bincode.accounts.into_iter().map(|(k, v)| (k, v.into())).collect(),
            contracts: cache_bincode
                .contracts
                .into_iter()
                .map(|(k, v)| (k, Bytecode::new_raw(v)))
                .collect(),
            logs: Default::default(),
            block_hashes: cache_bincode.block_hashes.into_iter().collect(),
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HashedPostStateBincode {
    pub accounts: Vec<(B256, Option<Account>)>,
    pub storages: Vec<(B256, HashedStorageBincode)>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HashedStorageBincode {
    pub wiped: bool,
    pub storage: Vec<(B256, U256)>,
}

impl From<HashedStorage> for HashedStorageBincode {
    fn from(storage: HashedStorage) -> Self {
        let mut storage_vec: Vec<_> = storage.storage.into_iter().collect();
        storage_vec.sort_unstable_by_key(|(k, _)| *k);
        Self { wiped: storage.wiped, storage: storage_vec }
    }
}

impl From<HashedPostState> for HashedPostStateBincode {
    fn from(state: HashedPostState) -> Self {
        let mut accounts: Vec<_> = state.accounts.into_iter().collect();
        accounts.sort_unstable_by_key(|(k, _)| *k);

        let mut storages: Vec<_> = state.storages.into_iter().map(|(k, v)| (k, v.into())).collect();
        storages.sort_unstable_by_key(|(k, _)| *k);

        Self { accounts, storages }
    }
}

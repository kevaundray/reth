#![allow(missing_docs)]

use alloc::collections::BTreeMap;
use alloy_primitives::{Address, Bytes, B256, U256};
use reth_revm::{
    db::{AccountState, Cache, DbAccount},
    primitives::{StorageKey, StorageValue},
    state::{AccountInfo, Bytecode},
};
use serde_with::{DeserializeAs, SerializeAs};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheBincode {
    pub accounts: BTreeMap<Address, DbAccountBincode>,
    pub contracts: BTreeMap<B256, Bytes>,
    pub block_hashes: BTreeMap<U256, B256>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DbAccountBincode {
    /// Basic account information.
    pub info: AccountInfo,
    /// If account is selfdestructed or newly created, storage will be cleared.
    pub account_state: AccountState,
    /// Storage slots
    pub storage: BTreeMap<StorageKey, StorageValue>,
}

impl From<DbAccount> for DbAccountBincode {
    fn from(account: DbAccount) -> Self {
        Self {
            info: account.info,
            account_state: account.account_state,
            storage: account.storage.into_iter().collect(),
        }
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

impl From<Cache> for CacheBincode {
    fn from(cache: Cache) -> Self {
        Self {
            accounts: cache.accounts.iter().map(|(k, v)| (*k, v.clone().into())).collect(),
            contracts: cache.contracts.iter().map(|(k, v)| (*k, v.original_bytes())).collect(),
            block_hashes: cache.block_hashes.iter().map(|(k, v)| (*k, *v)).collect(),
        }
    }
}

impl SerializeAs<Cache> for CacheBincode {
    fn serialize_as<S>(source: &Cache, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::Serialize;
        let cache_bincode: Self = source.clone().into();
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

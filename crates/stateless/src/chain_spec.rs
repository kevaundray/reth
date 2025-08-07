use core::{any::Any, fmt::Display};

use alloc::{boxed::Box, collections::btree_map::BTreeMap, vec::Vec};
use alloy_consensus::Header;
use alloy_eips::{eip7840::BlobParams, BlobScheduleBlobParams};
use alloy_genesis::Genesis;
use alloy_primitives::{Address, B256, U256};
use reth_chainspec::{
    BaseFeeParams, Chain, DepositContract, EthChainSpec, EthereumHardfork, EthereumHardforks,
    ForkCondition, ForkFilter, ForkId, Hardfork, Hardforks, Head,
};
use reth_evm::eth::spec::EthExecutorSpec;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

/// An Ethereum chain specification.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainSpec {
    /// The chain ID
    #[serde_as(as = "DisplayFromStr")]
    pub chain: Chain,
    /// The active hard forks and their activation conditions
    pub forks: BTreeMap<EthereumHardfork, ForkCondition>,
    /// The deposit contract deployed for `PoS`
    pub deposit_contract_address: Option<Address>,
    /// The settings passed for blob configurations for specific hardforks.
    #[serde(with = "BlobScheduleBlobParamsMirror")]
    pub blob_params: BlobScheduleBlobParams,
}

impl EthereumHardforks for ChainSpec {
    fn ethereum_fork_activation(&self, fork: EthereumHardfork) -> ForkCondition {
        self.forks.get(&fork).copied().unwrap_or_default()
    }
}

impl EthExecutorSpec for ChainSpec {
    fn deposit_contract_address(&self) -> Option<Address> {
        self.deposit_contract_address
    }
}

impl Hardforks for ChainSpec {
    fn fork<H: Hardfork>(&self, fork: H) -> ForkCondition {
        if let Some(eth_fork) = (&fork as &dyn Any).downcast_ref::<EthereumHardfork>() {
            self.ethereum_fork_activation(*eth_fork)
        } else {
            ForkCondition::Never
        }
    }

    fn forks_iter(&self) -> impl Iterator<Item = (&dyn Hardfork, ForkCondition)> {
        self.forks.iter().map(|(eth_fork, condition)| (eth_fork as &dyn Hardfork, *condition))
    }

    fn fork_id(&self, _head: &Head) -> ForkId {
        unimplemented!();
    }

    fn latest_fork_id(&self) -> ForkId {
        unimplemented!()
    }

    fn fork_filter(&self, _head: Head) -> ForkFilter {
        unimplemented!();
    }
}

impl EthChainSpec for ChainSpec {
    type Header = Header;

    fn chain(&self) -> Chain {
        self.chain
    }

    fn base_fee_params_at_block(&self, _: u64) -> BaseFeeParams {
        unimplemented!()
    }

    fn base_fee_params_at_timestamp(&self, _: u64) -> BaseFeeParams {
        unimplemented!()
    }

    fn blob_params_at_timestamp(&self, timestamp: u64) -> Option<alloy_eips::eip7840::BlobParams> {
        if let Some(blob_param) = self.blob_params.active_scheduled_params_at_timestamp(timestamp) {
            Some(*blob_param)
        } else if self.is_osaka_active_at_timestamp(timestamp) {
            Some(self.blob_params.osaka)
        } else if self.is_prague_active_at_timestamp(timestamp) {
            Some(self.blob_params.prague)
        } else if self.is_cancun_active_at_timestamp(timestamp) {
            Some(self.blob_params.cancun)
        } else {
            None
        }
    }

    fn deposit_contract(&self) -> Option<&DepositContract> {
        unimplemented!()
    }

    fn genesis_hash(&self) -> B256 {
        unimplemented!()
    }

    fn prune_delete_limit(&self) -> usize {
        unimplemented!()
    }

    fn display_hardforks(&self) -> Box<dyn Display> {
        unimplemented!()
    }

    fn genesis_header(&self) -> &Self::Header {
        unimplemented!()
    }

    fn genesis(&self) -> &Genesis {
        unimplemented!()
    }

    fn bootnodes(&self) -> Option<Vec<reth_network_peers::node_record::NodeRecord>> {
        unimplemented!()
    }

    fn final_paris_total_difficulty(&self) -> Option<U256> {
        if let ForkCondition::TTD { total_difficulty, .. } =
            self.ethereum_fork_activation(EthereumHardfork::Paris)
        {
            Some(total_difficulty)
        } else {
            None
        }
    }
}

// FIXME: this is a temporary workaround since `BlobScheduleBlobParams` doesn't support serde.
// This can be removed if we add support in `alloy-eips` which should be easy since it already has structs
// implementing serde guarded by a feature flag.
#[derive(Debug, Serialize, Deserialize)]
#[serde(remote = "BlobScheduleBlobParams")]
struct BlobScheduleBlobParamsMirror {
    pub cancun: BlobParams,
    pub prague: BlobParams,
    pub osaka: BlobParams,
    pub scheduled: Vec<(u64, BlobParams)>,
}

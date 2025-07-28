use core::fmt::Display;

use alloc::{boxed::Box, vec::Vec};
use alloy_consensus::Header;
use alloy_eips::BlobScheduleBlobParams;
use alloy_genesis::Genesis;
use alloy_primitives::{Address, B256, U256};
use reth_chainspec::{
    BaseFeeParams, Chain, ChainHardforks, DepositContract, EthChainSpec, EthereumHardfork,
    EthereumHardforks, ForkCondition, ForkFilter, ForkId, Hardfork, Hardforks, Head,
};
use reth_evm::eth::spec::EthExecutorSpec;

#[derive(Debug, Clone)]
pub struct ChainSpec {
    pub chain: Chain,
    pub hardforks: ChainHardforks,
    pub deposit_contract_address: Option<Address>,
    pub blob_params: BlobScheduleBlobParams,
}

impl EthereumHardforks for ChainSpec {
    fn ethereum_fork_activation(&self, fork: EthereumHardfork) -> ForkCondition {
        self.hardforks.get(fork).unwrap_or_default()
    }
}

impl EthExecutorSpec for ChainSpec {
    fn deposit_contract_address(&self) -> Option<Address> {
        self.deposit_contract_address
    }
}

impl Hardforks for ChainSpec {
    fn fork<H: Hardfork>(&self, fork: H) -> ForkCondition {
        self.hardforks.fork(fork)
    }

    fn forks_iter(&self) -> impl Iterator<Item = (&dyn Hardfork, ForkCondition)> {
        self.hardforks.forks_iter()
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

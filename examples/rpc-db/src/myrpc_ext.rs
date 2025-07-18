// Reth block related imports
use reth_ethereum::{provider::BlockReaderIdExt, rpc::eth::EthResult, Block};

// Rpc related imports
use jsonrpsee::proc_macros::rpc;

/// trait interface for a custom rpc namespace: `myrpcExt`
///
/// This defines an additional namespace where all methods are configured as trait functions.
#[rpc(server, namespace = "myrpcExt")]
pub trait MyRpcExtApi {
    /// Returns block 0.
    #[method(name = "customMethod")]
    fn custom_method(&self) -> EthResult<Option<Block>>;
}

/// The type that implements `myrpcExt` rpc namespace trait
pub struct MyRpcExt<Provider> {
    pub provider: Provider,
}

impl<Provider> MyRpcExtApiServer for MyRpcExt<Provider>
where
    Provider: BlockReaderIdExt<Block = Block> + 'static,
{
    /// Showcasing how to implement a custom rpc method
    /// using the provider.
    fn custom_method(&self) -> EthResult<Option<Block>> {
        let block = self.provider.block_by_number(0)?;
        Ok(block)
    }
}

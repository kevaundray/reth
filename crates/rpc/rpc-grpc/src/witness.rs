//! gRPC WitnessService implementation.

use crate::proto::{
    witness_service_server::WitnessService, ExecutionWitnessChunk, ExecutionWitnessHashRequest,
    ExecutionWitnessRequest, ExecutionWitnessResponse, WitnessField,
};
use alloy_consensus::BlockHeader;
use alloy_eips::BlockNumberOrTag;
use alloy_primitives::B256;
use alloy_rlp::Encodable;
use alloy_rpc_types_debug::ExecutionWitness;
use reth_evm::{execute::Executor, ConfigureEvm};
use reth_primitives_traits::RecoveredBlock;
use reth_revm::{db::State, witness::ExecutionWitnessRecord};
use reth_rpc_eth_api::helpers::{EthTransactions, TraceExt};
use reth_rpc_eth_types::EthApiError;
use reth_storage_api::{BlockReaderIdExt, HeaderProvider, ProviderBlock, StateProofProvider};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

/// gRPC server for execution witness data.
///
/// This provides a binary-efficient alternative to the JSON-RPC `debug_executionWitness`
/// endpoint, avoiding the overhead of hex-encoding large binary payloads.
#[derive(Debug)]
pub struct WitnessServer<Eth> {
    eth_api: Eth,
}

impl<Eth> WitnessServer<Eth> {
    /// Create a new `WitnessServer` with the given Eth API.
    pub fn new(eth_api: Eth) -> Self {
        Self { eth_api }
    }
}

impl<Eth> WitnessServer<Eth>
where
    Eth: EthTransactions + TraceExt + 'static,
    Eth::Provider: BlockReaderIdExt + HeaderProvider + StateProofProvider,
{
    /// Generate execution witness for a block.
    async fn generate_witness(
        &self,
        block: Arc<RecoveredBlock<ProviderBlock<Eth::Provider>>>,
    ) -> Result<ExecutionWitness, Eth::Error> {
        let block_number = block.header().number();

        let (mut exec_witness, lowest_block_number) = self
            .eth_api
            .spawn_with_state_at_block(block.header().parent_hash(), move |eth_api, mut db| {
                let block_executor = eth_api.evm_config().executor(&mut db);

                let mut witness_record = ExecutionWitnessRecord::default();

                let _ = block_executor
                    .execute_with_state_closure(&block, |statedb: &State<_>| {
                        witness_record.record_executed_state(statedb);
                    })
                    .map_err(|err| EthApiError::Internal(err.into()))?;

                let ExecutionWitnessRecord { hashed_state, codes, keys, lowest_block_number } =
                    witness_record;

                let state = db
                    .database
                    .0
                    .witness(Default::default(), hashed_state)
                    .map_err(EthApiError::from)?;

                Ok((
                    ExecutionWitness { state, codes, keys, ..Default::default() },
                    lowest_block_number,
                ))
            })
            .await?;

        // Collect block headers for BLOCKHASH opcode range
        let smallest = lowest_block_number.unwrap_or_else(|| block_number.saturating_sub(1));
        let range = smallest..block_number;

        exec_witness.headers = self
            .eth_api
            .provider()
            .headers_range(range)
            .map_err(EthApiError::from)?
            .into_iter()
            .map(|header| {
                let mut buf = Vec::new();
                header.encode(&mut buf);
                buf.into()
            })
            .collect();

        Ok(exec_witness)
    }

    /// Convert internal ExecutionWitness to proto response.
    fn to_proto_response(witness: ExecutionWitness) -> ExecutionWitnessResponse {
        ExecutionWitnessResponse {
            state: witness.state.into_iter().map(|b| b.to_vec()).collect(),
            codes: witness.codes.into_iter().map(|b| b.to_vec()).collect(),
            keys: witness.keys.into_iter().map(|b| b.to_vec()).collect(),
            headers: witness.headers.into_iter().map(|b| b.to_vec()).collect(),
        }
    }
}

#[tonic::async_trait]
impl<Eth> WitnessService for WitnessServer<Eth>
where
    Eth: EthTransactions + TraceExt + 'static,
    Eth::Provider: BlockReaderIdExt + HeaderProvider + StateProofProvider,
{
    async fn get_execution_witness(
        &self,
        request: Request<ExecutionWitnessRequest>,
    ) -> Result<Response<ExecutionWitnessResponse>, Status> {
        let block_number = request.into_inner().block_number;

        let block_id = if block_number < 0 {
            // Negative numbers map to special tags
            match block_number {
                -1 => BlockNumberOrTag::Latest,
                -2 => BlockNumberOrTag::Pending,
                -3 => BlockNumberOrTag::Safe,
                -4 => BlockNumberOrTag::Finalized,
                -5 => BlockNumberOrTag::Earliest,
                _ => return Err(Status::invalid_argument("invalid block number")),
            }
        } else {
            BlockNumberOrTag::Number(block_number as u64)
        };

        let block = self
            .eth_api
            .recovered_block(block_id.into())
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("block not found"))?;

        let witness = self
            .generate_witness(block)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(Self::to_proto_response(witness)))
    }

    async fn get_execution_witness_by_hash(
        &self,
        request: Request<ExecutionWitnessHashRequest>,
    ) -> Result<Response<ExecutionWitnessResponse>, Status> {
        let hash_bytes = request.into_inner().block_hash;

        if hash_bytes.len() != 32 {
            return Err(Status::invalid_argument("block_hash must be 32 bytes"));
        }

        let hash = B256::from_slice(&hash_bytes);

        let block = self
            .eth_api
            .recovered_block(hash.into())
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("block not found"))?;

        let witness = self
            .generate_witness(block)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(Self::to_proto_response(witness)))
    }

    type StreamExecutionWitnessStream = ReceiverStream<Result<ExecutionWitnessChunk, Status>>;

    async fn stream_execution_witness(
        &self,
        request: Request<ExecutionWitnessRequest>,
    ) -> Result<Response<Self::StreamExecutionWitnessStream>, Status> {
        let block_number = request.into_inner().block_number;

        let block_id = if block_number < 0 {
            match block_number {
                -1 => BlockNumberOrTag::Latest,
                -2 => BlockNumberOrTag::Pending,
                -3 => BlockNumberOrTag::Safe,
                -4 => BlockNumberOrTag::Finalized,
                -5 => BlockNumberOrTag::Earliest,
                _ => return Err(Status::invalid_argument("invalid block number")),
            }
        } else {
            BlockNumberOrTag::Number(block_number as u64)
        };

        let block = self
            .eth_api
            .recovered_block(block_id.into())
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("block not found"))?;

        let witness = self
            .generate_witness(block)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Stream chunks
        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            // Stream state chunks
            let state_len = witness.state.len();
            for (i, item) in witness.state.into_iter().enumerate() {
                let chunk = ExecutionWitnessChunk {
                    field: WitnessField::State.into(),
                    index: i as u32,
                    data: item.to_vec(),
                    is_last_for_field: i == state_len - 1,
                };
                if tx.send(Ok(chunk)).await.is_err() {
                    return;
                }
            }

            // Stream codes chunks
            let codes_len = witness.codes.len();
            for (i, item) in witness.codes.into_iter().enumerate() {
                let chunk = ExecutionWitnessChunk {
                    field: WitnessField::Codes.into(),
                    index: i as u32,
                    data: item.to_vec(),
                    is_last_for_field: i == codes_len - 1,
                };
                if tx.send(Ok(chunk)).await.is_err() {
                    return;
                }
            }

            // Stream keys chunks
            let keys_len = witness.keys.len();
            for (i, item) in witness.keys.into_iter().enumerate() {
                let chunk = ExecutionWitnessChunk {
                    field: WitnessField::Keys.into(),
                    index: i as u32,
                    data: item.to_vec(),
                    is_last_for_field: i == keys_len - 1,
                };
                if tx.send(Ok(chunk)).await.is_err() {
                    return;
                }
            }

            // Stream header chunks
            let headers_len = witness.headers.len();
            for (i, item) in witness.headers.into_iter().enumerate() {
                let chunk = ExecutionWitnessChunk {
                    field: WitnessField::Headers.into(),
                    index: i as u32,
                    data: item.to_vec(),
                    is_last_for_field: i == headers_len - 1,
                };
                if tx.send(Ok(chunk)).await.is_err() {
                    return;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

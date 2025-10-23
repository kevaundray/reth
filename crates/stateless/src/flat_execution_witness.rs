use reth_primitives_traits::Header;
use reth_revm::db::Cache;

#[derive(Debug, Clone)]

pub struct FlatExecutionWitness {
    pub state: Cache,
    pub parent_header: Header,
}

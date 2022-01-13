use std::{collections::BTreeMap, marker::PhantomData, sync::Arc};

use async_trait::async_trait;
use melnet::Request;
use novasmt::{CompressedProof, ContentAddrStore};
use themelio_stf::{AbbrBlock, BlockHeight, ConsensusProof, SealedState, Transaction};
use tmelcrypt::HashVal;

use crate::{NodeRequest, StateSummary, Substate};

/// This trait represents a server of Themelio's node protocol. Actual nodes should implement this.
pub trait NodeServer<C: ContentAddrStore>: Send + Sync {
    /// Broadcasts a transaction to the network
    fn send_tx(&self, state: melnet::NetState, tx: Transaction) -> anyhow::Result<()>;

    /// Gets an "abbreviated block"
    fn get_abbr_block(&self, height: BlockHeight) -> anyhow::Result<(AbbrBlock, ConsensusProof)>;

    /// Gets a state summary
    fn get_summary(&self) -> anyhow::Result<StateSummary>;

    /// Gets a full state
    fn get_state(&self, height: BlockHeight) -> anyhow::Result<SealedState<C>>;

    /// Gets an SMT branch
    fn get_smt_branch(
        &self,
        height: BlockHeight,
        elem: Substate,
        key: HashVal,
    ) -> anyhow::Result<(Vec<u8>, CompressedProof)>;

    /// Gets stakers
    fn get_stakers_raw(&self, height: BlockHeight) -> anyhow::Result<BTreeMap<HashVal, Vec<u8>>>;
}

/// This is a melnet responder that wraps a NodeServer.
pub struct NodeResponder<C: ContentAddrStore, S: NodeServer<C> + 'static> {
    server: Arc<S>,
    _p: PhantomData<C>,
}

impl<C: ContentAddrStore, S: NodeServer<C>> NodeResponder<C, S> {
    /// Creates a new NodeResponder from something that implements NodeServer.
    pub fn new(server: S) -> Self {
        Self {
            server: Arc::new(server),
            _p: Default::default(),
        }
    }
}

impl<C: ContentAddrStore, S: NodeServer<C>> Clone for NodeResponder<C, S> {
    fn clone(&self) -> Self {
        Self {
            server: self.server.clone(),
            _p: Default::default(),
        }
    }
}

#[async_trait]
impl<C: ContentAddrStore, S: NodeServer<C>> melnet::Endpoint<NodeRequest, Vec<u8>>
    for NodeResponder<C, S>
{
    async fn respond(&self, req: Request<NodeRequest>) -> anyhow::Result<Vec<u8>> {
        let state = req.state.clone();
        let server = self.server.clone();
        smol::unblock(move || match req.body.clone() {
            NodeRequest::SendTx(tx) => {
                server.send_tx(state, tx)?;
                Ok(vec![])
            }
            NodeRequest::GetSummary => Ok(stdcode::serialize(&server.get_summary()?)?),
            NodeRequest::GetAbbrBlock(height) => {
                Ok(stdcode::serialize(&server.get_abbr_block(height)?)?)
            }
            NodeRequest::GetSmtBranch(height, elem, key) => Ok(stdcode::serialize(
                &server.get_smt_branch(height, elem, key)?,
            )?),
            NodeRequest::GetStakersRaw(height) => {
                Ok(stdcode::serialize(&server.get_stakers_raw(height)?)?)
            }
            NodeRequest::GetPartialBlock(height, mut hvv) => {
                hvv.sort();
                let hvv = hvv;
                let ss = server.get_state(height)?;
                let mut blk = ss.to_block();
                blk.transactions
                    .retain(|h| hvv.binary_search(&h.hash_nosigs()).is_ok());
                Ok(stdcode::serialize(&blk)?)
            }
        })
        .await
    }
}

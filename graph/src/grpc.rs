use crate::store::GraphStore;
use astragraph_proto::astragraph::graph_service_server::GraphService;
use astragraph_proto::astragraph::{
    DriftPathRequest, DriftPathResponse, InsertNodeRequest, InsertNodeResponse, LinkEdgeRequest,
    LinkEdgeResponse,
};
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::info_span;

#[derive(Clone)]
pub struct GraphServiceImpl {
    store: Arc<RwLock<GraphStore>>,
}

impl GraphServiceImpl {
    pub fn new(store: Arc<RwLock<GraphStore>>) -> Self {
        Self { store }
    }
}

#[tonic::async_trait]
impl GraphService for GraphServiceImpl {
    async fn insert_node(
        &self,
        request: Request<InsertNodeRequest>,
    ) -> Result<Response<InsertNodeResponse>, Status> {
        let _span = info_span!("graph.insert_node").entered();
        let payload = request.into_inner();
        let node = payload
            .node
            .ok_or_else(|| Status::invalid_argument("missing node"))?;
        let node_id = node.id.clone();
        let mut store = self.store.write().map_err(|_| Status::internal("store"))?;
        store
            .insert_node(node)
            .map_err(|_| Status::internal("insert failed"))?;
        Ok(Response::new(InsertNodeResponse { node_id }))
    }

    async fn link_edge(
        &self,
        request: Request<LinkEdgeRequest>,
    ) -> Result<Response<LinkEdgeResponse>, Status> {
        let _span = info_span!("graph.link_edge").entered();
        let payload = request.into_inner();
        let edge = payload
            .edge
            .ok_or_else(|| Status::invalid_argument("missing edge"))?;
        let mut store = self.store.write().map_err(|_| Status::internal("store"))?;
        store
            .link_edge(edge)
            .map_err(|_| Status::internal("link failed"))?;
        Ok(Response::new(LinkEdgeResponse {}))
    }

    type StreamNodesStream = Pin<Box<dyn Stream<Item = Result<InsertNodeResponse, Status>> + Send>>;

    async fn stream_nodes(
        &self,
        request: Request<tonic::Streaming<InsertNodeRequest>>,
    ) -> Result<Response<Self::StreamNodesStream>, Status> {
        let _span = info_span!("graph.stream_nodes").entered();
        let mut stream = request.into_inner();
        let store = self.store.clone();
        let (tx, rx) = mpsc::channel(8);

        tokio::spawn(async move {
            while let Ok(Some(payload)) = stream.message().await {
                let node = match payload.node {
                    Some(node) => node,
                    None => {
                        let _ = tx.send(Err(Status::invalid_argument("missing node"))).await;
                        continue;
                    }
                };
                let node_id = node.id.clone();
                let insert_result = {
                    let mut store = match store.write() {
                        Ok(store) => store,
                        Err(_) => {
                            let _ = tx.try_send(Err(Status::internal("store")));
                            continue;
                        }
                    };
                    store
                        .insert_node(node)
                        .map(|_| InsertNodeResponse { node_id })
                        .map_err(|_| Status::internal("insert failed"))
                };

                match insert_result {
                    Ok(response) => {
                        let _ = tx.send(Ok(response)).await;
                    }
                    Err(status) => {
                        let _ = tx.send(Err(status)).await;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    type StreamEdgesStream = Pin<Box<dyn Stream<Item = Result<LinkEdgeResponse, Status>> + Send>>;

    async fn stream_edges(
        &self,
        request: Request<tonic::Streaming<LinkEdgeRequest>>,
    ) -> Result<Response<Self::StreamEdgesStream>, Status> {
        let _span = info_span!("graph.stream_edges").entered();
        let mut stream = request.into_inner();
        let store = self.store.clone();
        let (tx, rx) = mpsc::channel(8);

        tokio::spawn(async move {
            while let Ok(Some(payload)) = stream.message().await {
                let edge = match payload.edge {
                    Some(edge) => edge,
                    None => {
                        let _ = tx.send(Err(Status::invalid_argument("missing edge"))).await;
                        continue;
                    }
                };
                let link_result = {
                    let mut store = match store.write() {
                        Ok(store) => store,
                        Err(_) => {
                            let _ = tx.try_send(Err(Status::internal("store")));
                            continue;
                        }
                    };
                    store
                        .link_edge(edge)
                        .map(|_| LinkEdgeResponse {})
                        .map_err(|_| Status::internal("link failed"))
                };

                match link_result {
                    Ok(response) => {
                        let _ = tx.send(Ok(response)).await;
                    }
                    Err(status) => {
                        let _ = tx.send(Err(status)).await;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    async fn get_drift_path(
        &self,
        request: Request<DriftPathRequest>,
    ) -> Result<Response<DriftPathResponse>, Status> {
        let _span = info_span!("graph.get_drift_path").entered();
        let payload = request.into_inner();
        let store = self.store.read().map_err(|_| Status::internal("store"))?;
        let path = store
            .drift_path(&payload.graph_id, &payload.node_id, payload.threshold)
            .ok_or_else(|| Status::not_found("path"))?;
        let origin = path.first().cloned().unwrap_or_default();
        Ok(Response::new(DriftPathResponse {
            node_ids: path,
            origin,
        }))
    }
}

use std::sync::Arc;
use dashmap::DashMap;
use uuid::Uuid;
use crate::config::Config;
use crate::orm::OrmDatabase;
use shellwego_network::QuinnServer;
use shellwego_network::quinn::server::AgentConnection;

pub struct AppState {
    pub config: Config,
    pub db: Arc<OrmDatabase>,
    pub agents: DashMap<Uuid, AgentConnection>,
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Arc<Self>> {
        let db = Arc::new(OrmDatabase::connect(&config.database_url).await?);

        Ok(Arc::new(Self {
            config,
            db,
            agents: DashMap::new(),
        }))
    }
}

impl axum::extract::FromRef<Arc<AppState>> for Arc<AppState> {
    fn from_ref(state: &Arc<AppState>) -> Arc<AppState> {
        state.clone()
    }
}

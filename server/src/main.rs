pub mod adapters;
pub mod core;
pub mod domain;

use std::sync::Arc;

use tonic::transport::Server;

use crate::adapters::r#in::grpc::proto::relay_server::RelayServer;
use crate::adapters::r#in::grpc::relay_service::RelayService;
use crate::core::initializers::redis::MailboxStore;
use crate::core::initializers::settings::{AppEnv, Settings};
use crate::domain::services::mailbox::Mailbox;

#[tokio::main]
async fn main() {
    let settings = match Settings::new() {
        Ok(settings) => settings,
        Err(e) => {
            eprintln!("Failed to load configuration: {e}");
            std::process::exit(1);
        }
    };

    common::logging::init(
        &settings.logging.logging_level,
        settings.environment == AppEnv::Production,
    );
    tracing::info!("Booting relay server...");

    let mailbox_store = match MailboxStore::new(&settings.redis) {
        Ok(store) => store,
        Err(e) => {
            tracing::error!("Failed to initialize Redis Sentinel client: {e}");
            std::process::exit(1);
        }
    };

    let mailbox = Arc::new(Mailbox::new(mailbox_store));
    let relay_service = RelayService::new(mailbox);

    let addr = match settings.server.bind_addr.parse() {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Invalid bind address '{}': {e}", settings.server.bind_addr);
            std::process::exit(1);
        }
    };

    tracing::info!("Relay server listening on {addr}");

    if let Err(e) = Server::builder()
        .add_service(RelayServer::new(relay_service))
        .serve(addr)
        .await
    {
        tracing::error!(error = %e, debug = ?e, "gRPC server terminated");
        std::process::exit(1);
    }
}

use std::sync::Arc;

use tonic::transport::Server;

use server::adapters::r#in::grpc::proto::relay_server::RelayServer;
use server::adapters::r#in::grpc::relay_service::RelayService;
use server::adapters::out::push::FcmClient;
use server::core::initializers::logger::init;
use server::core::initializers::redis::MailboxStore;
use server::core::initializers::settings::Settings;
use server::domain::services::mailbox::Mailbox;
use server::domain::services::push;

#[tokio::main]
async fn main() {
    let settings = match Settings::new() {
        Ok(settings) => settings,
        Err(e) => {
            eprintln!("Failed to load configuration: {e}");
            std::process::exit(1);
        }
    };

    init(&settings);
    tracing::info!("Booting relay server...");

    let mailbox_store = match MailboxStore::new(&settings.redis) {
        Ok(store) => store,
        Err(e) => {
            tracing::error!(error = %e, "Failed to initialize Redis Sentinel client");
            std::process::exit(1);
        }
    };

    tracing::info!(
        sentinel_nodes = ?settings.redis.sentinel_nodes,
        service_name = %settings.redis.service_name,
        "Redis Sentinel client configured"
    );

    let mailbox = Arc::new(Mailbox::new(mailbox_store));
    let relay_service = RelayService::new(mailbox.clone());

    match settings.push.service_account_path.as_deref() {
        Some(path) => match FcmClient::from_service_account_file(path).await {
            Ok(client) => {
                tracing::info!("FCM dispatch enabled");
                tokio::spawn(push::run(mailbox.clone(), Arc::new(client)));
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to initialize FCM client, push notifications disabled");
            }
        },
        None => {
            tracing::warn!("No FCM service account configured, push notifications disabled");
        }
    };

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

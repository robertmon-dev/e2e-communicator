use std::sync::Arc;

use tonic::transport::Server;

use server::adapters::r#in::grpc::auth_service::AuthService;
use server::adapters::r#in::grpc::proto::identity::auth_server::AuthServer;
use server::adapters::r#in::grpc::proto::identity::register_server::RegisterServer;
use server::adapters::r#in::grpc::proto::relay_server::RelayServer;
use server::adapters::r#in::grpc::register_service::RegisterService;
use server::adapters::r#in::grpc::relay_service::RelayService;
use server::adapters::out::push::FcmClient;
use server::core::initializers::logger::init;
use server::core::initializers::redis::MailboxStore;
use server::core::initializers::settings::Settings;
use server::domain::services::mailbox::Mailbox;
use server::domain::services::push;
use server::domain::services::registry::Registry;

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

    let mailbox = Arc::new(Mailbox::new(mailbox_store.clone()));
    let registry = Arc::new(Registry::new(mailbox_store, settings.auth.clone()));
    let relay_service = RelayService::new(mailbox.clone(), registry.clone());
    let register_service = RegisterService::new(registry.clone(), mailbox.clone());
    let auth_service = AuthService::new(registry);

    tracing::info!(
        session_ttl_secs = settings.auth.session_ttl_secs,
        challenge_ttl_secs = settings.auth.challenge_ttl_secs,
        link_token_ttl_secs = settings.auth.link_token_ttl_secs,
        "Device registry and authentication configured"
    );

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
        .add_service(RegisterServer::new(register_service))
        .add_service(AuthServer::new(auth_service))
        .serve(addr)
        .await
    {
        tracing::error!(error = %e, debug = ?e, "gRPC server terminated");
        std::process::exit(1);
    }
}

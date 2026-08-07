#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Zero-knowledge relay: this server only ever handles `common::envelope::EncryptedEnvelope`
    // for routing/storage. It never sees decrypted message content.
    tracing::info!("Relay server scaffold — not yet implemented");
}

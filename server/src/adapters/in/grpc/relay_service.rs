use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::adapters::r#in::grpc::proto;
use crate::adapters::r#in::grpc::proto::relay_server::Relay;
use crate::adapters::r#in::grpc::session::{ensure_claims, resolve_session};
use crate::adapters::r#in::grpc::utils::{
    envelope_from_proto, envelope_to_proto, mailbox_status, parse_uuid,
};
use crate::core::initializers::redis::MailboxStore;
use crate::domain::dto::errors::MailboxError;
use crate::domain::services::mailbox::Mailbox;
use crate::domain::services::registry::Registry;

const INBOUND_CHANNEL_CAPACITY: usize = 32;
const DEFAULT_SESSION_RECHECK_EVERY: u32 = 12;
const BLOCKING_RECONNECT_DELAY: Duration = Duration::from_millis(500);
const MAX_CONSECUTIVE_READ_FAILURES: u32 = 5;

pub struct RelayService {
    mailbox: Arc<Mailbox>,
    registry: Arc<Registry>,
    session_recheck_every: u32,
}

impl RelayService {
    pub fn new(mailbox: Arc<Mailbox>, registry: Arc<Registry>) -> Self {
        Self {
            mailbox,
            registry,
            session_recheck_every: DEFAULT_SESSION_RECHECK_EVERY,
        }
    }

    pub fn with_session_recheck_every(mut self, iterations: u32) -> Self {
        self.session_recheck_every = iterations.max(1);
        self
    }
}

#[tonic::async_trait]
impl Relay for RelayService {
    async fn send_envelope(
        &self,
        request: Request<proto::SendEnvelopeRequest>,
    ) -> Result<Response<proto::SendEnvelopeResponse>, Status> {
        let session = resolve_session(&request, &self.registry).await?;

        if request.get_ref().device_id.is_empty() {
            return Err(Status::invalid_argument("missing device_id"));
        }
        let device_id = parse_uuid(&request.get_ref().device_id, "device_id")?;

        let proto_envelope = request
            .get_ref()
            .envelope
            .clone()
            .ok_or_else(|| Status::invalid_argument("missing envelope"))?;
        let envelope = envelope_from_proto(proto_envelope)?;

        ensure_claims(&session, envelope.sender_id, device_id)?;

        tracing::debug!(
            envelope_id = %envelope.id,
            chat_id = %envelope.chat_id,
            sender_id = %envelope.sender_id,
            recipient_id = %envelope.recipient_id,
            "send_envelope received"
        );

        self.mailbox
            .fan_out(&envelope)
            .await
            .map_err(|err| mailbox_status(err, "fan_out"))?;

        Ok(Response::new(proto::SendEnvelopeResponse {}))
    }

    type SubscribeEnvelopesStream =
        Pin<Box<dyn Stream<Item = Result<proto::InboundEnvelope, Status>> + Send + 'static>>;

    async fn subscribe_envelopes(
        &self,
        request: Request<proto::SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeEnvelopesStream>, Status> {
        let session = resolve_session(&request, &self.registry).await?;
        let user_id = parse_uuid(&request.get_ref().user_id, "user_id")?;
        let device_id = parse_uuid(&request.get_ref().device_id, "device_id")?;
        ensure_claims(&session, user_id, device_id)?;

        let (user_id, device_id) = (session.user_id, session.device_id);
        let token = session.token;
        let recheck_every = self.session_recheck_every;

        let mailbox = self.mailbox.clone();
        let registry = self.registry.clone();

        mailbox
            .register_device(user_id, device_id)
            .await
            .map_err(|err| mailbox_status(err, "register_device"))?;

        let mut conn = mailbox
            .blocking_connection()
            .await
            .map_err(|err| mailbox_status(err, "blocking_connection"))?;

        tracing::info!(%user_id, %device_id, "client subscribed");

        let (tx, rx) = mpsc::channel(INBOUND_CHANNEL_CAPACITY);

        tokio::spawn(async move {
            let mut iteration: u32 = 0;
            let mut failures: u32 = 0;

            loop {
                iteration = iteration.wrapping_add(1);

                if iteration.is_multiple_of(recheck_every)
                    && !matches!(registry.session(&token).await, Ok(Some(_)))
                {
                    tracing::info!(%user_id, %device_id, "subscription session no longer valid");
                    let _ = tx
                        .send(Err(Status::unauthenticated("session expired")))
                        .await;
                    return;
                }

                if let Err(err) = mailbox.heartbeat(user_id, device_id).await {
                    tracing::warn!(error = %err, %user_id, %device_id, "heartbeat failed");
                }

                let batch = match mailbox.read_next(&mut conn, user_id, device_id).await {
                    Ok(batch) => {
                        failures = 0;
                        batch
                    }
                    Err(MailboxError::Redis(err)) if MailboxStore::needs_reconnect(&err) => {
                        failures += 1;

                        if failures >= MAX_CONSECUTIVE_READ_FAILURES {
                            let _ = tx
                                .send(Err(mailbox_status(MailboxError::Redis(err), "read_next")))
                                .await;
                            return;
                        }

                        tracing::warn!(
                            error = %err,
                            %user_id,
                            %device_id,
                            attempt = failures,
                            "blocking read failed; replacing subscriber connection"
                        );

                        tokio::time::sleep(BLOCKING_RECONNECT_DELAY).await;

                        match mailbox.blocking_connection().await {
                            Ok(fresh) => conn = fresh,
                            Err(err) => tracing::warn!(
                                error = %err,
                                %user_id,
                                %device_id,
                                "could not reopen subscriber connection"
                            ),
                        }

                        continue;
                    }
                    Err(err) => {
                        let _ = tx.send(Err(mailbox_status(err, "read_next"))).await;
                        return;
                    }
                };

                if !batch.is_empty() {
                    tracing::debug!(%user_id, %device_id, count = batch.len(), "delivering batch");
                }

                for (delivery_token, envelope) in batch {
                    let inbound = proto::InboundEnvelope {
                        envelope: Some(envelope_to_proto(envelope)),
                        delivery_token,
                    };

                    if tx.send(Ok(inbound)).await.is_err() {
                        tracing::info!(%user_id, %device_id, "subscriber stream closed");
                        return;
                    }
                }
            }
        });

        let stream: Self::SubscribeEnvelopesStream = Box::pin(ReceiverStream::new(rx));
        Ok(Response::new(stream))
    }

    async fn ack_envelope(
        &self,
        request: Request<proto::AckEnvelopeRequest>,
    ) -> Result<Response<proto::AckEnvelopeResponse>, Status> {
        let session = resolve_session(&request, &self.registry).await?;
        let user_id = parse_uuid(&request.get_ref().user_id, "user_id")?;
        let device_id = parse_uuid(&request.get_ref().device_id, "device_id")?;
        ensure_claims(&session, user_id, device_id)?;

        let (user_id, device_id) = (session.user_id, session.device_id);
        let req = request.into_inner();

        self.mailbox
            .ack(user_id, device_id, &req.delivery_token)
            .await
            .map_err(|err| mailbox_status(err, "ack"))?;

        tracing::debug!(%user_id, %device_id, delivery_token = %req.delivery_token, "envelope acked");

        Ok(Response::new(proto::AckEnvelopeResponse {}))
    }

    async fn register_push_token(
        &self,
        request: Request<proto::RegisterPushTokenRequest>,
    ) -> Result<Response<proto::RegisterPushTokenResponse>, Status> {
        let session = resolve_session(&request, &self.registry).await?;
        let user_id = parse_uuid(&request.get_ref().user_id, "user_id")?;
        let device_id = parse_uuid(&request.get_ref().device_id, "device_id")?;
        ensure_claims(&session, user_id, device_id)?;

        let (user_id, device_id) = (session.user_id, session.device_id);
        let req = request.into_inner();

        if req.fcm_token.is_empty() {
            return Err(Status::invalid_argument("fcm_token must not be empty"));
        }

        self.mailbox
            .register_push_token(user_id, device_id, &req.fcm_token)
            .await
            .map_err(|err| mailbox_status(err, "register_push_token"))?;

        tracing::info!(%user_id, %device_id, "push token registered");

        Ok(Response::new(proto::RegisterPushTokenResponse {}))
    }
}

use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::adapters::r#in::grpc::proto;
use crate::adapters::r#in::grpc::proto::relay_server::Relay;
use crate::adapters::r#in::grpc::utils::{
    envelope_from_proto, envelope_to_proto, mailbox_status, parse_uuid,
};
use crate::domain::services::mailbox::Mailbox;

const INBOUND_CHANNEL_CAPACITY: usize = 32;

pub struct RelayService {
    mailbox: Arc<Mailbox>,
}

impl RelayService {
    pub fn new(mailbox: Arc<Mailbox>) -> Self {
        Self { mailbox }
    }
}

#[tonic::async_trait]
impl Relay for RelayService {
    async fn send_envelope(
        &self,
        request: Request<proto::SendEnvelopeRequest>,
    ) -> Result<Response<proto::SendEnvelopeResponse>, Status> {
        let proto_envelope = request
            .into_inner()
            .envelope
            .ok_or_else(|| Status::invalid_argument("missing envelope"))?;

        let envelope = envelope_from_proto(proto_envelope)?;

        tracing::debug!(
            envelope_id = %envelope.id,
            chat_id = %envelope.chat_id,
            sender_id = %envelope.sender_id,
            recipient_id = %envelope.recipient_id,
            "send_envelope received"
        );

        self.mailbox.fan_out(&envelope).await.map_err(|err| {
            tracing::error!(error = %err, envelope_id = %envelope.id, "fan_out failed");
            mailbox_status(err)
        })?;

        Ok(Response::new(proto::SendEnvelopeResponse {}))
    }

    type SubscribeEnvelopesStream =
        Pin<Box<dyn Stream<Item = Result<proto::InboundEnvelope, Status>> + Send + 'static>>;

    async fn subscribe_envelopes(
        &self,
        request: Request<proto::SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeEnvelopesStream>, Status> {
        let req = request.into_inner();
        let user_id = parse_uuid(&req.user_id, "user_id")?;
        let device_id = parse_uuid(&req.device_id, "device_id")?;

        let mailbox = self.mailbox.clone();
        mailbox
            .register_device(user_id, device_id)
            .await
            .map_err(|err| {
                tracing::error!(error = %err, %user_id, %device_id, "register_device failed");
                mailbox_status(err)
            })?;

        tracing::info!(%user_id, %device_id, "client subscribed");

        let (tx, rx) = mpsc::channel(INBOUND_CHANNEL_CAPACITY);

        tokio::spawn(async move {
            loop {
                let batch = match mailbox.read_next(user_id, device_id).await {
                    Ok(batch) => batch,
                    Err(err) => {
                        tracing::error!(error = %err, %user_id, %device_id, "read_next failed");
                        let _ = tx.send(Err(mailbox_status(err))).await;
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
        let req = request.into_inner();
        let user_id = parse_uuid(&req.user_id, "user_id")?;
        let device_id = parse_uuid(&req.device_id, "device_id")?;

        self.mailbox
            .ack(user_id, device_id, &req.delivery_token)
            .await
            .map_err(|err| {
                tracing::error!(error = %err, %user_id, %device_id, "ack failed");
                mailbox_status(err)
            })?;

        tracing::debug!(%user_id, %device_id, delivery_token = %req.delivery_token, "envelope acked");

        Ok(Response::new(proto::AckEnvelopeResponse {}))
    }
}

use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::adapters::r#in::grpc::proto;
use crate::adapters::r#in::grpc::proto::relay_server::Relay;
use crate::domain::dto::errors::MailboxError;
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

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|_| Status::invalid_argument(format!("invalid {field}")))
}

fn mailbox_status(err: MailboxError) -> Status {
    Status::unavailable(err.to_string())
}

fn envelope_from_proto(
    proto_envelope: proto::EncryptedEnvelope,
) -> Result<common::envelope::EncryptedEnvelope, Status> {
    let event_type = common::envelope::EventType::try_from(proto_envelope.event_type)
        .map_err(|_| Status::invalid_argument("invalid event_type"))?;

    let created_at = chrono::DateTime::from_timestamp_millis(proto_envelope.created_at_unix_ms)
        .ok_or_else(|| Status::invalid_argument("invalid created_at_unix_ms"))?;

    Ok(common::envelope::EncryptedEnvelope {
        id: parse_uuid(&proto_envelope.id, "id")?,
        chat_id: parse_uuid(&proto_envelope.chat_id, "chat_id")?,
        sender_id: parse_uuid(&proto_envelope.sender_id, "sender_id")?,
        recipient_id: parse_uuid(&proto_envelope.recipient_id, "recipient_id")?,
        created_at,
        event_type,
        ciphertext: proto_envelope.ciphertext,
    })
}

fn envelope_to_proto(envelope: common::envelope::EncryptedEnvelope) -> proto::EncryptedEnvelope {
    proto::EncryptedEnvelope {
        id: envelope.id.to_string(),
        chat_id: envelope.chat_id.to_string(),
        sender_id: envelope.sender_id.to_string(),
        recipient_id: envelope.recipient_id.to_string(),
        created_at_unix_ms: envelope.created_at.timestamp_millis(),
        event_type: envelope.event_type as i32,
        ciphertext: envelope.ciphertext,
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

        self.mailbox
            .fan_out(&envelope)
            .await
            .map_err(mailbox_status)?;

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
            .map_err(mailbox_status)?;

        let (tx, rx) = mpsc::channel(INBOUND_CHANNEL_CAPACITY);

        tokio::spawn(async move {
            loop {
                let batch = match mailbox.read_next(user_id, device_id).await {
                    Ok(batch) => batch,
                    Err(err) => {
                        let _ = tx.send(Err(mailbox_status(err))).await;
                        return;
                    }
                };

                for (delivery_token, envelope) in batch {
                    let inbound = proto::InboundEnvelope {
                        envelope: Some(envelope_to_proto(envelope)),
                        delivery_token,
                    };

                    if tx.send(Ok(inbound)).await.is_err() {
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
            .map_err(mailbox_status)?;

        Ok(Response::new(proto::AckEnvelopeResponse {}))
    }
}

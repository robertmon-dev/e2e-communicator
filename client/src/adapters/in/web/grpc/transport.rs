use std::sync::Arc;

use tokio_stream::StreamExt;
use tonic::transport::Channel;
use uuid::Uuid;

use common::envelope::{EncryptedEnvelope, EventType};

use crate::adapters::r#in::web::grpc::proto;
use crate::adapters::r#in::web::grpc::proto::relay_client::RelayClient;
use crate::adapters::r#in::web::grpc::utils::{authorized, receive_error, transport_error};
use crate::domain::dto::errors::TransportError;
use crate::domain::dto::outbox::OutboxEntry;
use crate::domain::services::inbox::{EnvelopeSubscription, InboundStream};
use crate::domain::services::relay::EnvelopeTransport;
use crate::domain::state::identity::IdentityState;
use crate::domain::state::session::SessionState;

pub struct GrpcRelayClient {
    client: RelayClient<Channel>,
    session: Arc<SessionState>,
    identity: Arc<IdentityState>,
}

impl GrpcRelayClient {
    pub async fn connect(
        endpoint: String,
        session: Arc<SessionState>,
        identity: Arc<IdentityState>,
    ) -> Result<Self, TransportError> {
        let client = RelayClient::connect(endpoint)
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;

        Ok(Self {
            client,
            session,
            identity,
        })
    }

    fn local_device_id(&self) -> String {
        self.identity
            .registered()
            .map(|identity| identity.device_id.to_string())
            .unwrap_or_default()
    }
}

fn parse_uuid(raw: &str) -> Result<Uuid, TransportError> {
    Uuid::parse_str(raw).map_err(|e| TransportError::ReceiveFailed(e.to_string()))
}

fn envelope_from_proto(
    proto_envelope: proto::EncryptedEnvelope,
) -> Result<EncryptedEnvelope, TransportError> {
    let event_type = EventType::try_from(proto_envelope.event_type)
        .map_err(|e| TransportError::ReceiveFailed(e.to_string()))?;

    let created_at = chrono::DateTime::from_timestamp_millis(proto_envelope.created_at_unix_ms)
        .ok_or_else(|| TransportError::ReceiveFailed("invalid created_at_unix_ms".to_string()))?;

    Ok(EncryptedEnvelope {
        id: parse_uuid(&proto_envelope.id)?,
        chat_id: parse_uuid(&proto_envelope.chat_id)?,
        sender_id: parse_uuid(&proto_envelope.sender_id)?,
        recipient_id: parse_uuid(&proto_envelope.recipient_id)?,
        created_at,
        event_type,
        ciphertext: proto_envelope.ciphertext,
    })
}

impl EnvelopeTransport for GrpcRelayClient {
    async fn send(&self, entry: &OutboxEntry) -> Result<(), TransportError> {
        let mut client = self.client.clone();

        let request = proto::SendEnvelopeRequest {
            envelope: Some(proto::EncryptedEnvelope {
                id: entry.id.to_string(),
                chat_id: entry.chat_id.to_string(),
                sender_id: entry.sender_id.to_string(),
                recipient_id: entry.recipient_id.to_string(),
                created_at_unix_ms: entry.created_at.timestamp_millis(),
                event_type: entry.event_type.clone() as i32,
                ciphertext: entry.ciphertext.clone(),
            }),
            device_id: self.local_device_id(),
        };

        client
            .send_envelope(authorized(&self.session, request))
            .await
            .map_err(transport_error)?;

        Ok(())
    }
}

impl EnvelopeSubscription for GrpcRelayClient {
    async fn subscribe(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<InboundStream, TransportError> {
        let mut client = self.client.clone();

        let request = proto::SubscribeRequest {
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
        };

        let response = client
            .subscribe_envelopes(authorized(&self.session, request))
            .await
            .map_err(receive_error)?;

        let stream = response.into_inner().map(|item| {
            let inbound = item.map_err(receive_error)?;
            let proto_envelope = inbound
                .envelope
                .ok_or_else(|| TransportError::ReceiveFailed("missing envelope".to_string()))?;

            Ok((inbound.delivery_token, envelope_from_proto(proto_envelope)?))
        });

        let boxed: InboundStream = Box::pin(stream);
        Ok(boxed)
    }

    async fn ack(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        delivery_token: &str,
    ) -> Result<(), TransportError> {
        let mut client = self.client.clone();

        let request = proto::AckEnvelopeRequest {
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            delivery_token: delivery_token.to_string(),
        };

        client
            .ack_envelope(authorized(&self.session, request))
            .await
            .map_err(receive_error)?;

        Ok(())
    }
}

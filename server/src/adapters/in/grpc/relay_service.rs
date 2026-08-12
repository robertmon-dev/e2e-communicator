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

mod fixtures;

use std::sync::Arc;

use server::adapters::r#in::grpc::proto;
use server::adapters::r#in::grpc::proto::relay_server::Relay;
use server::adapters::r#in::grpc::relay_service::RelayService;
use server::domain::services::mailbox::Mailbox;
use tonic::Request;
use uuid::Uuid;

fn test_service() -> (RelayService, Arc<Mailbox>) {
    let mailbox = fixtures::mailbox();
    (RelayService::new(mailbox.clone()), mailbox)
}

#[tokio::test]
async fn register_push_token_stores_the_token() {
    let (service, mailbox) = test_service();
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    let request = Request::new(proto::RegisterPushTokenRequest {
        user_id: user_id.to_string(),
        device_id: device_id.to_string(),
        fcm_token: "test-token".to_string(),
    });

    service.register_push_token(request).await.unwrap();

    let token = mailbox.get_push_token(user_id, device_id).await.unwrap();
    assert_eq!(token, Some("test-token".to_string()));
}

#[tokio::test]
async fn register_push_token_rejects_empty_token() {
    let (service, _) = test_service();

    let request = Request::new(proto::RegisterPushTokenRequest {
        user_id: Uuid::new_v4().to_string(),
        device_id: Uuid::new_v4().to_string(),
        fcm_token: String::new(),
    });

    assert!(service.register_push_token(request).await.is_err());
}

#[tokio::test]
async fn register_push_token_rejects_invalid_user_id() {
    let (service, _) = test_service();

    let request = Request::new(proto::RegisterPushTokenRequest {
        user_id: "not-a-uuid".to_string(),
        device_id: Uuid::new_v4().to_string(),
        fcm_token: "tok".to_string(),
    });

    assert!(service.register_push_token(request).await.is_err());
}

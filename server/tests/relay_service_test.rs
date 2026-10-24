mod fixtures;

use std::sync::Arc;

use fixtures::{TestDevice, with_bearer};
use server::adapters::r#in::grpc::proto;
use server::adapters::r#in::grpc::proto::relay_server::Relay;
use server::adapters::r#in::grpc::relay_service::RelayService;
use server::domain::services::mailbox::Mailbox;
use server::domain::services::registry::Registry;
use tonic::{Code, Request};
use uuid::Uuid;

fn test_service() -> (RelayService, Arc<Mailbox>, Arc<Registry>) {
    let mailbox = fixtures::mailbox();
    let registry = fixtures::registry();
    (
        RelayService::new(mailbox.clone(), registry.clone()),
        mailbox,
        registry,
    )
}

#[tokio::test]
async fn register_push_token_stores_the_token() {
    let (service, mailbox, registry) = test_service();
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let session = fixtures::login(&registry, &device).await;

    let request = with_bearer(
        Request::new(proto::RegisterPushTokenRequest {
            user_id: device.user_id.to_string(),
            device_id: device.device_id.to_string(),
            fcm_token: "test-token".to_string(),
        }),
        &session.token,
    );

    service.register_push_token(request).await.unwrap();

    let token = mailbox
        .get_push_token(device.user_id, device.device_id)
        .await
        .unwrap();
    assert_eq!(token, Some("test-token".to_string()));
}

#[tokio::test]
async fn register_push_token_without_a_session_is_unauthenticated() {
    let (service, _, _) = test_service();

    let request = Request::new(proto::RegisterPushTokenRequest {
        user_id: Uuid::new_v4().to_string(),
        device_id: Uuid::new_v4().to_string(),
        fcm_token: "test-token".to_string(),
    });

    let status = service.register_push_token(request).await.unwrap_err();
    assert_eq!(status.code(), Code::Unauthenticated);
}

#[tokio::test]
async fn register_push_token_rejects_empty_token() {
    let (service, _, registry) = test_service();
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let session = fixtures::login(&registry, &device).await;

    let request = with_bearer(
        Request::new(proto::RegisterPushTokenRequest {
            user_id: device.user_id.to_string(),
            device_id: device.device_id.to_string(),
            fcm_token: String::new(),
        }),
        &session.token,
    );

    let status = service.register_push_token(request).await.unwrap_err();
    assert_eq!(status.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn register_push_token_rejects_invalid_user_id() {
    let (service, _, registry) = test_service();
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let session = fixtures::login(&registry, &device).await;

    let request = with_bearer(
        Request::new(proto::RegisterPushTokenRequest {
            user_id: "not-a-uuid".to_string(),
            device_id: device.device_id.to_string(),
            fcm_token: "tok".to_string(),
        }),
        &session.token,
    );

    let status = service.register_push_token(request).await.unwrap_err();
    assert_eq!(status.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn register_push_token_without_a_session_is_unauthenticated_even_with_a_garbage_body() {
    let (service, _, _) = test_service();

    let request = Request::new(proto::RegisterPushTokenRequest {
        user_id: "not-a-uuid".to_string(),
        device_id: "also-not-a-uuid".to_string(),
        fcm_token: String::new(),
    });

    let status = service.register_push_token(request).await.unwrap_err();
    assert_eq!(status.code(), Code::Unauthenticated);
}

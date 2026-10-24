mod fixtures;

use fixtures::{TestDevice, with_bearer};
use redis::AsyncCommands;
use server::adapters::r#in::grpc::auth_service::AuthService;
use server::adapters::r#in::grpc::proto;
use server::adapters::r#in::grpc::proto::identity as identity_proto;
use server::adapters::r#in::grpc::proto::identity::auth_server::Auth;
use server::adapters::r#in::grpc::proto::identity::register_server::Register;
use server::adapters::r#in::grpc::proto::relay_server::Relay;
use server::adapters::r#in::grpc::register_service::RegisterService;
use server::adapters::r#in::grpc::relay_service::RelayService;
use server::adapters::r#in::grpc::session::require_session;
use tokio_stream::StreamExt;
use tonic::{Code, Request};
use uuid::Uuid;

fn keys_proto(device: &TestDevice) -> identity_proto::DeviceKeys {
    identity_proto::DeviceKeys {
        signing_key: device.keys.signing_key.to_vec(),
        agreement_key: device.keys.agreement_key.to_vec(),
    }
}

fn register_request(device: &TestDevice) -> identity_proto::RegisterDeviceRequest {
    identity_proto::RegisterDeviceRequest {
        user_id: device.user_id.to_string(),
        device_id: device.device_id.to_string(),
        keys: Some(keys_proto(device)),
        self_proof: device.proof().to_vec(),
    }
}

async fn grpc_login(auth: &AuthService, device: &TestDevice) -> String {
    let nonce = auth
        .challenge(Request::new(identity_proto::ChallengeRequest {
            user_id: device.user_id.to_string(),
            device_id: device.device_id.to_string(),
        }))
        .await
        .unwrap()
        .into_inner()
        .nonce;
    let nonce: [u8; 32] = nonce.try_into().unwrap();

    auth.authenticate(Request::new(identity_proto::AuthenticateRequest {
        user_id: device.user_id.to_string(),
        device_id: device.device_id.to_string(),
        signature: device.sign_challenge(&nonce).to_vec(),
    }))
    .await
    .unwrap()
    .into_inner()
    .token
}

#[tokio::test]
async fn register_then_challenge_then_authenticate_over_grpc() {
    let registry = fixtures::registry();
    let register = RegisterService::new(registry.clone(), fixtures::mailbox());
    let auth = AuthService::new(registry.clone());
    let device = TestDevice::new(Uuid::new_v4());

    register
        .register_device(Request::new(register_request(&device)))
        .await
        .unwrap();

    let token = grpc_login(&auth, &device).await;
    let session = registry.session(&token).await.unwrap().unwrap();
    assert_eq!(session.device_id, device.device_id);
}

#[tokio::test]
async fn registering_a_taken_user_id_is_already_exists() {
    let registry = fixtures::registry();
    let register = RegisterService::new(registry.clone(), fixtures::mailbox());
    let user_id = Uuid::new_v4();
    let first = TestDevice::new(user_id);
    let second = TestDevice::new(user_id);

    register
        .register_device(Request::new(register_request(&first)))
        .await
        .unwrap();
    let status = register
        .register_device(Request::new(register_request(&second)))
        .await
        .unwrap_err();

    assert_eq!(status.code(), Code::AlreadyExists);
}

#[tokio::test]
async fn a_bad_challenge_signature_is_unauthenticated() {
    let registry = fixtures::registry();
    let auth = AuthService::new(registry.clone());
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;

    auth.challenge(Request::new(identity_proto::ChallengeRequest {
        user_id: device.user_id.to_string(),
        device_id: device.device_id.to_string(),
    }))
    .await
    .unwrap();

    let status = auth
        .authenticate(Request::new(identity_proto::AuthenticateRequest {
            user_id: device.user_id.to_string(),
            device_id: device.device_id.to_string(),
            signature: vec![0u8; 64],
        }))
        .await
        .unwrap_err();

    assert_eq!(status.code(), Code::Unauthenticated);
}

#[tokio::test]
async fn link_flow_over_grpc_adds_a_device() {
    let registry = fixtures::registry();
    let register = RegisterService::new(registry.clone(), fixtures::mailbox());
    let auth = AuthService::new(registry.clone());
    let first = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &first).await;
    let token = grpc_login(&auth, &first).await;

    let link = register
        .create_link_token(with_bearer(
            Request::new(identity_proto::CreateLinkTokenRequest {
                user_id: first.user_id.to_string(),
                device_id: first.device_id.to_string(),
            }),
            &token,
        ))
        .await
        .unwrap()
        .into_inner();

    let second = TestDevice::new(first.user_id);
    let linked = register
        .link_device(Request::new(identity_proto::LinkDeviceRequest {
            token: link.token,
            device_id: second.device_id.to_string(),
            keys: Some(keys_proto(&second)),
            self_proof: second.proof().to_vec(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(linked.user_id, first.user_id.to_string());

    let devices = register
        .lookup_devices(with_bearer(
            Request::new(identity_proto::LookupDevicesRequest {
                user_id: first.user_id.to_string(),
                device_id: first.device_id.to_string(),
                target_user_id: first.user_id.to_string(),
            }),
            &token,
        ))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(devices.devices.len(), 2);
}

#[tokio::test]
async fn create_link_token_without_a_session_is_unauthenticated() {
    let registry = fixtures::registry();
    let register = RegisterService::new(registry.clone(), fixtures::mailbox());
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;

    let status = register
        .create_link_token(Request::new(identity_proto::CreateLinkTokenRequest {
            user_id: device.user_id.to_string(),
            device_id: device.device_id.to_string(),
        }))
        .await
        .unwrap_err();

    assert_eq!(status.code(), Code::Unauthenticated);
}

#[tokio::test]
async fn require_session_rejects_missing_unknown_and_mismatched() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let session = fixtures::login(&registry, &device).await;

    let missing = require_session(
        &Request::new(()),
        &registry,
        device.user_id,
        device.device_id,
    )
    .await
    .unwrap_err();
    assert_eq!(missing.code(), Code::Unauthenticated);

    let unknown = require_session(
        &with_bearer(Request::new(()), "not-a-session"),
        &registry,
        device.user_id,
        device.device_id,
    )
    .await
    .unwrap_err();
    assert_eq!(unknown.code(), Code::Unauthenticated);

    let mismatched = require_session(
        &with_bearer(Request::new(()), &session.token),
        &registry,
        Uuid::new_v4(),
        device.device_id,
    )
    .await
    .unwrap_err();
    assert_eq!(mismatched.code(), Code::PermissionDenied);

    let ok = require_session(
        &with_bearer(Request::new(()), &session.token),
        &registry,
        device.user_id,
        device.device_id,
    )
    .await
    .unwrap();
    assert_eq!(ok.token, session.token);
}

#[tokio::test]
async fn send_envelope_with_a_foreign_sender_is_permission_denied() {
    let registry = fixtures::registry();
    let relay = RelayService::new(fixtures::mailbox(), registry.clone());
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let session = fixtures::login(&registry, &device).await;

    let request = with_bearer(
        Request::new(proto::SendEnvelopeRequest {
            envelope: Some(proto::EncryptedEnvelope {
                id: Uuid::new_v4().to_string(),
                chat_id: Uuid::new_v4().to_string(),
                sender_id: Uuid::new_v4().to_string(),
                recipient_id: Uuid::new_v4().to_string(),
                created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
                event_type: 0,
                ciphertext: vec![1, 2, 3],
            }),
            device_id: device.device_id.to_string(),
        }),
        &session.token,
    );

    let status = relay.send_envelope(request).await.unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn send_envelope_with_the_session_owner_as_sender_succeeds() {
    let registry = fixtures::registry();
    let relay = RelayService::new(fixtures::mailbox(), registry.clone());
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let session = fixtures::login(&registry, &device).await;

    let request = with_bearer(
        Request::new(proto::SendEnvelopeRequest {
            envelope: Some(proto::EncryptedEnvelope {
                id: Uuid::new_v4().to_string(),
                chat_id: Uuid::new_v4().to_string(),
                sender_id: device.user_id.to_string(),
                recipient_id: Uuid::new_v4().to_string(),
                created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
                event_type: 0,
                ciphertext: vec![1, 2, 3],
            }),
            device_id: device.device_id.to_string(),
        }),
        &session.token,
    );

    relay.send_envelope(request).await.unwrap();
}

#[tokio::test]
async fn send_envelope_without_a_session_is_unauthenticated_even_with_a_broken_body() {
    let registry = fixtures::registry();
    let relay = RelayService::new(fixtures::mailbox(), registry.clone());

    let status = relay
        .send_envelope(Request::new(proto::SendEnvelopeRequest {
            envelope: None,
            device_id: String::new(),
        }))
        .await
        .unwrap_err();

    assert_eq!(status.code(), Code::Unauthenticated);
}

#[tokio::test]
async fn send_envelope_with_an_empty_device_id_is_invalid_argument() {
    let registry = fixtures::registry();
    let relay = RelayService::new(fixtures::mailbox(), registry.clone());
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let session = fixtures::login(&registry, &device).await;

    let request = with_bearer(
        Request::new(proto::SendEnvelopeRequest {
            envelope: None,
            device_id: String::new(),
        }),
        &session.token,
    );

    let status = relay.send_envelope(request).await.unwrap_err();
    assert_eq!(status.code(), Code::InvalidArgument);
    assert_eq!(status.message(), "missing device_id");
}

#[tokio::test]
async fn revoking_own_device_over_grpc_kills_the_session() {
    let registry = fixtures::registry();
    let register = RegisterService::new(registry.clone(), fixtures::mailbox());
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let session = fixtures::login(&registry, &device).await;

    register
        .revoke_device(with_bearer(
            Request::new(identity_proto::RevokeDeviceRequest {
                user_id: device.user_id.to_string(),
                device_id: device.device_id.to_string(),
                target_device_id: device.device_id.to_string(),
            }),
            &session.token,
        ))
        .await
        .unwrap();

    assert!(registry.session(&session.token).await.unwrap().is_none());
}

#[tokio::test]
async fn revoking_a_device_over_grpc_also_clears_its_mailbox_state() {
    let registry = fixtures::registry();
    let mailbox = fixtures::mailbox();
    let register = RegisterService::new(registry.clone(), mailbox.clone());
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let session = fixtures::login(&registry, &device).await;

    mailbox
        .register_device(device.user_id, device.device_id)
        .await
        .unwrap();
    mailbox
        .register_push_token(device.user_id, device.device_id, "fcm-token")
        .await
        .unwrap();
    mailbox
        .heartbeat(device.user_id, device.device_id)
        .await
        .unwrap();

    register
        .revoke_device(with_bearer(
            Request::new(identity_proto::RevokeDeviceRequest {
                user_id: device.user_id.to_string(),
                device_id: device.device_id.to_string(),
                target_device_id: device.device_id.to_string(),
            }),
            &session.token,
        ))
        .await
        .unwrap();

    assert!(
        !mailbox
            .is_online(device.user_id, device.device_id)
            .await
            .unwrap()
    );
    assert_eq!(
        mailbox
            .get_push_token(device.user_id, device.device_id)
            .await
            .unwrap(),
        None
    );

    let mut conn = fixtures::store().connection().await.unwrap();
    let members: Vec<String> = conn
        .smembers(format!("devices:{}", device.user_id))
        .await
        .unwrap();
    assert!(!members.contains(&device.device_id.to_string()));
}

#[tokio::test]
async fn linking_a_device_id_with_different_keys_over_grpc_is_already_exists() {
    let registry = fixtures::registry();
    let register = RegisterService::new(registry.clone(), fixtures::mailbox());
    let auth = AuthService::new(registry.clone());
    let first = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &first).await;
    let token = grpc_login(&auth, &first).await;

    let link_request = |bearer: &str| {
        with_bearer(
            Request::new(identity_proto::CreateLinkTokenRequest {
                user_id: first.user_id.to_string(),
                device_id: first.device_id.to_string(),
            }),
            bearer,
        )
    };

    let link = register
        .create_link_token(link_request(&token))
        .await
        .unwrap()
        .into_inner();

    let second = TestDevice::new(first.user_id);
    register
        .link_device(Request::new(identity_proto::LinkDeviceRequest {
            token: link.token,
            device_id: second.device_id.to_string(),
            keys: Some(keys_proto(&second)),
            self_proof: second.proof().to_vec(),
        }))
        .await
        .unwrap();

    let fresh = register
        .create_link_token(link_request(&token))
        .await
        .unwrap()
        .into_inner();

    let mut rotated = TestDevice::new(first.user_id);
    rotated.device_id = second.device_id;

    let status = register
        .link_device(Request::new(identity_proto::LinkDeviceRequest {
            token: fresh.token,
            device_id: rotated.device_id.to_string(),
            keys: Some(keys_proto(&rotated)),
            self_proof: rotated.proof().to_vec(),
        }))
        .await
        .unwrap_err();

    assert_eq!(status.code(), Code::AlreadyExists);
}

#[tokio::test]
async fn an_open_subscription_ends_once_its_session_is_revoked() {
    let registry = fixtures::registry();
    let mailbox = fixtures::mailbox();
    let relay = RelayService::new(mailbox.clone(), registry.clone()).with_session_recheck_every(1);
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let session = fixtures::login(&registry, &device).await;

    let mut stream = relay
        .subscribe_envelopes(with_bearer(
            Request::new(proto::SubscribeRequest {
                user_id: device.user_id.to_string(),
                device_id: device.device_id.to_string(),
            }),
            &session.token,
        ))
        .await
        .unwrap()
        .into_inner();

    registry
        .revoke_device(device.user_id, device.device_id)
        .await
        .unwrap();

    mailbox
        .fan_out(&common::envelope::EncryptedEnvelope {
            id: Uuid::new_v4(),
            chat_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            recipient_id: device.user_id,
            created_at: chrono::Utc::now(),
            event_type: common::envelope::EventType::Message,
            ciphertext: vec![1, 2, 3],
        })
        .await
        .unwrap();

    let mut ended_unauthenticated = false;
    for _ in 0..3 {
        let item = tokio::time::timeout(std::time::Duration::from_secs(15), stream.next())
            .await
            .expect("subscription did not terminate")
            .expect("stream ended without a status");

        if let Err(status) = item {
            assert_eq!(status.code(), Code::Unauthenticated);
            ended_unauthenticated = true;
            break;
        }
    }

    assert!(
        ended_unauthenticated,
        "stream never reported the revocation"
    );
}

#[tokio::test]
async fn an_open_subscription_keeps_delivering_while_its_session_lives() {
    const RECHECK_EVERY: u32 = 3;
    const ROUNDS: u32 = RECHECK_EVERY + 1;

    let registry = fixtures::registry();
    let mailbox = fixtures::mailbox();
    let relay = RelayService::new(mailbox.clone(), registry.clone())
        .with_session_recheck_every(RECHECK_EVERY);
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let session = fixtures::login(&registry, &device).await;

    let mut stream = relay
        .subscribe_envelopes(with_bearer(
            Request::new(proto::SubscribeRequest {
                user_id: device.user_id.to_string(),
                device_id: device.device_id.to_string(),
            }),
            &session.token,
        ))
        .await
        .unwrap()
        .into_inner();

    for round in 0..ROUNDS {
        mailbox
            .fan_out(&common::envelope::EncryptedEnvelope {
                id: Uuid::new_v4(),
                chat_id: Uuid::new_v4(),
                sender_id: Uuid::new_v4(),
                recipient_id: device.user_id,
                created_at: chrono::Utc::now(),
                event_type: common::envelope::EventType::Message,
                ciphertext: vec![round as u8],
            })
            .await
            .unwrap();

        let item = tokio::time::timeout(std::time::Duration::from_secs(15), stream.next())
            .await
            .unwrap_or_else(|_| panic!("round {round} timed out"))
            .expect("stream ended early");

        let inbound = item.unwrap_or_else(|status| {
            panic!(
                "round {round} failed with {:?}: {}",
                status.code(),
                status.message()
            )
        });

        mailbox
            .ack(device.user_id, device.device_id, &inbound.delivery_token)
            .await
            .unwrap();
    }

    assert!(
        registry.session(&session.token).await.unwrap().is_some(),
        "the session must still be live"
    );
}

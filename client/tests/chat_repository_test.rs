mod fixtures;

use std::collections::BTreeMap;

use uuid::Uuid;

use client::adapters::out::persistance::repositories::chat::ChatRepository;
use client::adapters::out::persistance::repositories::chat::create::Create;
use client::adapters::out::persistance::repositories::chat::utils::ChatRestrictions;
use client::domain::dto::chat::{Chat, ChatType};
use client::domain::dto::filter_chat::ListChatsFilter;
use client::domain::dto::utils::PaginationMeta;
use fixtures::setup_db;

fn pagination_for(count: u64) -> PaginationMeta {
    PaginationMeta {
        current_page: 0,
        per_page: count.max(1),
        total_pages: 1,
        total_count: count,
    }
}

fn restrictions_for(chat_id: Uuid) -> ChatRestrictions {
    ChatRestrictions {
        restricted_chat_ids: Some(vec![chat_id]),
        restricted_user_ids: None,
        historical_memberships: None,
        only_active: Some(true),
    }
}

#[tokio::test]
async fn inactive_chat_is_hidden_from_active_only_listing_but_still_gettable() {
    let db = setup_db().await;
    let repository = ChatRepository::new(db.clone());

    let chat = Chat::new(
        ChatType::Direct,
        None,
        BTreeMap::new(),
        BTreeMap::new(),
        None,
        false,
    );
    let chat_id = chat.id;

    repository.create(chat).await.unwrap();

    let listed = repository
        .list(
            ListChatsFilter::default(),
            pagination_for(1),
            restrictions_for(chat_id),
        )
        .await
        .unwrap();

    assert!(listed.is_empty());

    let fetched = repository.get(chat_id).await.unwrap();
    assert_eq!(fetched.id, chat_id);
    assert!(!fetched.is_active);
}

#[tokio::test]
async fn active_chat_is_present_in_active_only_listing() {
    let db = setup_db().await;
    let repository = ChatRepository::new(db.clone());

    let chat = Chat::new(
        ChatType::Direct,
        None,
        BTreeMap::new(),
        BTreeMap::new(),
        None,
        true,
    );
    let chat_id = chat.id;

    repository.create(chat).await.unwrap();

    let listed = repository
        .list(
            ListChatsFilter::default(),
            pagination_for(1),
            restrictions_for(chat_id),
        )
        .await
        .unwrap();

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, chat_id);
    assert!(listed[0].is_active);
}

#[tokio::test]
async fn with_id_preserves_given_id_and_activity_flag() {
    let fixed_id = Uuid::new_v4();

    let chat = Chat::with_id(fixed_id, ChatType::Direct, None, BTreeMap::new(), false);

    assert_eq!(chat.id, fixed_id);
    assert!(!chat.is_active);
}

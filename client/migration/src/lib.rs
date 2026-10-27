use sea_orm_migration::prelude::*;

pub mod connection;

pub mod m20260702_000001_create_attachment_table;
pub mod m20260702_000002_create_chat_table;
pub mod m20260702_000003_create_chat_history_table;
pub mod m20260702_000004_create_chat_member_table;
pub mod m20260702_000005_create_file_metadata_table;
pub mod m20260702_000006_create_messages_table;
pub mod m20260702_000007_create_message_history_table;
pub mod m20260702_000008_create_profile_photo_history_table;
pub mod m20260702_000009_create_reaction_table;
pub mod m20260702_000010_create_reaction_history_table;
pub mod m20260702_000011_create_users_table;
pub mod m20260702_000012_create_user_overlay_history_table;
pub mod m20260801_000013_create_encryption_keys;
pub mod m20260806_000014_add_user_id_to_key;
pub mod m20260806_000015_create_outbox_table;
pub mod m20260806_000016_add_retry_fields_to_outbox;
pub mod m20260806_000017_add_scheduled_for_to_outbox;
pub mod m20260806_000018_create_status_schedule_table;
pub mod m20260808_000019_create_local_identity_table;
pub mod m20261004_000020_create_device_key_table;
pub mod m20261024_000021_add_signing_keys;
pub mod m20261026_000022_create_user_identity_table;
pub mod m20261026_000023_add_device_cert;
pub mod m20261026_000024_rebuild_key_per_device;
pub mod m20261026_000025_create_peer_device_table;
pub mod m20261026_000026_create_invitation_table;
pub mod m20261026_000027_rebuild_outbox_per_device;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260702_000011_create_users_table::Migration),
            Box::new(m20260702_000002_create_chat_table::Migration),
            Box::new(m20260702_000005_create_file_metadata_table::Migration),
            Box::new(m20260702_000012_create_user_overlay_history_table::Migration),
            Box::new(m20260702_000008_create_profile_photo_history_table::Migration),
            Box::new(m20260702_000003_create_chat_history_table::Migration),
            Box::new(m20260702_000006_create_messages_table::Migration),
            Box::new(m20260702_000004_create_chat_member_table::Migration),
            Box::new(m20260702_000007_create_message_history_table::Migration),
            Box::new(m20260702_000009_create_reaction_table::Migration),
            Box::new(m20260702_000001_create_attachment_table::Migration),
            Box::new(m20260702_000010_create_reaction_history_table::Migration),
            Box::new(m20260801_000013_create_encryption_keys::Migration),
            Box::new(m20260806_000014_add_user_id_to_key::Migration),
            Box::new(m20260806_000015_create_outbox_table::Migration),
            Box::new(m20260806_000016_add_retry_fields_to_outbox::Migration),
            Box::new(m20260806_000017_add_scheduled_for_to_outbox::Migration),
            Box::new(m20260806_000018_create_status_schedule_table::Migration),
            Box::new(m20260808_000019_create_local_identity_table::Migration),
            Box::new(m20261004_000020_create_device_key_table::Migration),
            Box::new(m20261024_000021_add_signing_keys::Migration),
            Box::new(m20261026_000022_create_user_identity_table::Migration),
            Box::new(m20261026_000023_add_device_cert::Migration),
            Box::new(m20261026_000024_rebuild_key_per_device::Migration),
            Box::new(m20261026_000027_rebuild_outbox_per_device::Migration),
            Box::new(m20261026_000025_create_peer_device_table::Migration),
            Box::new(m20261026_000026_create_invitation_table::Migration),
        ]
    }
}

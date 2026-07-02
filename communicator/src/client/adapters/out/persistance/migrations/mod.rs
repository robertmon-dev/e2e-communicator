use sea_orm_migration::prelude::*;

pub mod m20260702_000001_create_attachment_table;
pub mod m20260702_000002_create_chat_table;
pub mod m20260702_000003_create_chat_history_table;
pub mod m20260702_000004_create_chat_member_table;
pub mod m20260702_000005_create_file_metadata_table;
pub mod m20260702_000006_create_messages_table;
pub mod m20260702_000007_create_message_history_table;
pub mod m20260702_000008_create_profile_photo_history_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260702_000002_create_chat_table::Migration),
            Box::new(m20260702_000003_create_chat_history_table::Migration),
            Box::new(m20260702_000004_create_chat_member_table::Migration),
            Box::new(m20260702_000001_create_attachment_table::Migration),
        ]
    }
}

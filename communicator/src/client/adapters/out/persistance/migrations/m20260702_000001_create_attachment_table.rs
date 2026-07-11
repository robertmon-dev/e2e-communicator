use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Attachment::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Attachment::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Attachment::ChatId).uuid().not_null())
                    .col(ColumnDef::new(Attachment::AuthorId).uuid().not_null())
                    .col(
                        ColumnDef::new(Attachment::MessageHistoryId)
                            .uuid()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Attachment::AttachmentLink).string())
                    .col(
                        ColumnDef::new(Attachment::AttachmentType)
                            .integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Attachment::FileMetadataId).uuid().not_null())
                    .col(
                        ColumnDef::new(Attachment::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Attachment::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-attachment-chat_id")
                            .from(Attachment::Table, Attachment::ChatId)
                            .to(Chat::Table, Chat::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-attachment-author_id")
                            .from(Attachment::Table, Attachment::AuthorId)
                            .to(User::Table, User::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-attachment-message_history_id")
                            .from(Attachment::Table, Attachment::MessageHistoryId)
                            .to(MessageHistory::Table, MessageHistory::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-attachment-file_metadata_id")
                            .from(Attachment::Table, Attachment::FileMetadataId)
                            .to(FileMetadata::Table, FileMetadata::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Attachment::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Attachment {
    Table,
    Id,
    ChatId,
    AuthorId,
    MessageHistoryId,
    AttachmentLink,
    AttachmentType,
    FileMetadataId,
    UpdatedAt,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Chat {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum MessageHistory {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum FileMetadata {
    Table,
    Id,
}

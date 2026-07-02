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
    CreatedAt,
}

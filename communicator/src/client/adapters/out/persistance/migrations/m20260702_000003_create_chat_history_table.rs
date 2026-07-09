use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ChatHistory::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ChatHistory::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ChatHistory::ChatId).uuid().not_null())
                    .col(ColumnDef::new(ChatHistory::Name).string().not_null())
                    .col(
                        ColumnDef::new(ChatHistory::ChatType)
                            .tiny_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(ChatHistory::ChatTheme).string())
                    .col(
                        ColumnDef::new(ChatHistory::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ChatHistory::IsActive)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-chat_history-chat_id")
                            .from(ChatHistory::Table, ChatHistory::ChatId)
                            .to(Chat::Table, Chat::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ChatHistory::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ChatHistory {
    Table,
    Id,
    ChatId,
    Name,
    ChatType,
    ChatTheme,
    CreatedAt,
    IsActive,
}

#[derive(DeriveIden)]
enum Chat {
    Table,
    Id,
}

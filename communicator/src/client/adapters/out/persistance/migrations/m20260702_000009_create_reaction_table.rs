use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Reaction::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Reaction::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Reaction::ChatId).uuid().not_null())
                    .col(ColumnDef::new(Reaction::MessageHistoryId).uuid().not_null())
                    .col(ColumnDef::new(Reaction::AuthorId).uuid().not_null())
                    .col(
                        ColumnDef::new(Reaction::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-reaction-chat_id")
                            .from(Reaction::Table, Reaction::ChatId)
                            .to(Chat::Table, Chat::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-reaction-message_history_id")
                            .from(Reaction::Table, Reaction::MessageHistoryId)
                            .to(MessageHistory::Table, MessageHistory::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-reaction-author_id")
                            .from(Reaction::Table, Reaction::AuthorId)
                            .to(User::Table, User::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Reaction::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Reaction {
    Table,
    Id,
    ChatId,
    MessageHistoryId,
    AuthorId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Chat {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum MessageHistory {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}

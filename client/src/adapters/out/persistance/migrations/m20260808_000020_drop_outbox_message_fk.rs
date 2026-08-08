use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("PRAGMA foreign_keys=OFF;").await?;

        manager
            .create_table(
                Table::create()
                    .table(OutboxNew::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(OutboxNew::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(OutboxNew::MessageId).uuid().not_null())
                    .col(ColumnDef::new(OutboxNew::ChatId).uuid().not_null())
                    .col(ColumnDef::new(OutboxNew::SenderId).uuid().not_null())
                    .col(ColumnDef::new(OutboxNew::RecipientId).uuid().not_null())
                    .col(ColumnDef::new(OutboxNew::EventType).integer().not_null())
                    .col(ColumnDef::new(OutboxNew::Ciphertext).binary().not_null())
                    .col(ColumnDef::new(OutboxNew::Status).integer().not_null())
                    .col(
                        ColumnDef::new(OutboxNew::Attempts)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(OutboxNew::NextRetryAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(OutboxNew::ScheduledFor).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(OutboxNew::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OutboxNew::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-outbox_new-chat_id")
                            .from(OutboxNew::Table, OutboxNew::ChatId)
                            .to(Chat::Table, Chat::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-outbox_new-sender_id")
                            .from(OutboxNew::Table, OutboxNew::SenderId)
                            .to(Users::Table, Users::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-outbox_new-recipient_id")
                            .from(OutboxNew::Table, OutboxNew::RecipientId)
                            .to(Users::Table, Users::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        db.execute_unprepared(
            "INSERT INTO outbox_new (id, message_id, chat_id, sender_id, recipient_id, event_type, ciphertext, status, attempts, next_retry_at, scheduled_for, created_at, updated_at) \
             SELECT id, message_id, chat_id, sender_id, recipient_id, event_type, ciphertext, status, attempts, next_retry_at, scheduled_for, created_at, updated_at FROM outbox;",
        )
        .await?;

        manager
            .drop_table(Table::drop().table(Outbox::Table).to_owned())
            .await?;

        db.execute_unprepared("ALTER TABLE outbox_new RENAME TO outbox;")
            .await?;
        db.execute_unprepared("PRAGMA foreign_keys=ON;").await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Migration(
            "down migration not supported for outbox message_id FK removal".to_string(),
        ))
    }
}

#[derive(DeriveIden)]
enum OutboxNew {
    Table,
    Id,
    MessageId,
    ChatId,
    SenderId,
    RecipientId,
    EventType,
    Ciphertext,
    Status,
    Attempts,
    NextRetryAt,
    ScheduledFor,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Outbox {
    Table,
}

#[derive(DeriveIden)]
enum Chat {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

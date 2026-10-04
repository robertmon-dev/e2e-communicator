use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Outbox::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Outbox::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Outbox::MessageId).uuid().not_null())
                    .col(ColumnDef::new(Outbox::ChatId).uuid().not_null())
                    .col(ColumnDef::new(Outbox::SenderId).uuid().not_null())
                    .col(ColumnDef::new(Outbox::RecipientId).uuid().not_null())
                    .col(ColumnDef::new(Outbox::EventType).integer().not_null())
                    .col(ColumnDef::new(Outbox::Ciphertext).binary().not_null())
                    .col(ColumnDef::new(Outbox::Status).integer().not_null())
                    .col(
                        ColumnDef::new(Outbox::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Outbox::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-outbox-message_id")
                            .from(Outbox::Table, Outbox::MessageId)
                            .to(Messages::Table, Messages::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-outbox-chat_id")
                            .from(Outbox::Table, Outbox::ChatId)
                            .to(Chat::Table, Chat::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-outbox-sender_id")
                            .from(Outbox::Table, Outbox::SenderId)
                            .to(Users::Table, Users::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-outbox-recipient_id")
                            .from(Outbox::Table, Outbox::RecipientId)
                            .to(Users::Table, Users::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-outbox-status-created_at")
                    .table(Outbox::Table)
                    .col(Outbox::Status)
                    .col(Outbox::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Outbox::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Outbox {
    Table,
    Id,
    MessageId,
    ChatId,
    SenderId,
    RecipientId,
    EventType,
    Ciphertext,
    Status,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Messages {
    Table,
    Id,
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

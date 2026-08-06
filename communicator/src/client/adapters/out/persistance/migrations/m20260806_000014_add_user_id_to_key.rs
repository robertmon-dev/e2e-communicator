use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Key::Table).to_owned())
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Key::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Key::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Key::PublicKey).binary().not_null())
                    .col(ColumnDef::new(Key::EncryptedPrivateKey).binary().null())
                    .col(ColumnDef::new(Key::ChatId).uuid().not_null())
                    .col(ColumnDef::new(Key::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(Key::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Key::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-key-chat_id")
                            .from(Key::Table, Key::ChatId)
                            .to(Chat::Table, Chat::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-key-user_id")
                            .from(Key::Table, Key::UserId)
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
                    .name("idx-key-chat_id-user_id")
                    .table(Key::Table)
                    .col(Key::ChatId)
                    .col(Key::UserId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Key::Table).to_owned())
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Key::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Key::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Key::PublicKey).binary().not_null())
                    .col(ColumnDef::new(Key::EncryptedPrivateKey).binary().not_null())
                    .col(ColumnDef::new(Key::ChatId).uuid().not_null())
                    .col(
                        ColumnDef::new(Key::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Key::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-key-chat_id")
                            .from(Key::Table, Key::ChatId)
                            .to(Chat::Table, Chat::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Key {
    Table,
    Id,
    PublicKey,
    EncryptedPrivateKey,
    ChatId,
    UserId,
    CreatedAt,
    UpdatedAt,
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

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Invitation::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Invitation::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Invitation::ChatId).uuid().not_null())
                    .col(ColumnDef::new(Invitation::InviterUserId).uuid().not_null())
                    .col(ColumnDef::new(Invitation::InviteeUserId).uuid().not_null())
                    .col(ColumnDef::new(Invitation::Direction).integer().not_null())
                    .col(ColumnDef::new(Invitation::Status).integer().not_null())
                    .col(
                        ColumnDef::new(Invitation::ExpectedFingerprint)
                            .binary()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Invitation::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Invitation::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Invitation::RespondedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-invitation-chat_id")
                            .from(Invitation::Table, Invitation::ChatId)
                            .to(Chat::Table, Chat::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-invitation-inviter_user_id")
                            .from(Invitation::Table, Invitation::InviterUserId)
                            .to(Users::Table, Users::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-invitation-invitee_user_id")
                            .from(Invitation::Table, Invitation::InviteeUserId)
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
                    .name("idx-invitation-chat_id-invitee_user_id")
                    .table(Invitation::Table)
                    .col(Invitation::ChatId)
                    .col(Invitation::InviteeUserId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Invitation::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Invitation {
    Table,
    Id,
    ChatId,
    InviterUserId,
    InviteeUserId,
    Direction,
    Status,
    ExpectedFingerprint,
    CreatedAt,
    UpdatedAt,
    RespondedAt,
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

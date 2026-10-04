use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ChatMember::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ChatMember::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ChatMember::Role).tiny_integer().not_null())
                    .col(ColumnDef::new(ChatMember::ChatHistoryId).uuid().not_null())
                    .col(ColumnDef::new(ChatMember::UserId).uuid().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-chat_member-chat_history_id")
                            .from(ChatMember::Table, ChatMember::ChatHistoryId)
                            .to(ChatHistory::Table, ChatHistory::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-chat_member-user_id")
                            .from(ChatMember::Table, ChatMember::UserId)
                            .to(Users::Table, Users::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ChatMember::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ChatMember {
    Table,
    Id,
    Role,
    ChatHistoryId,
    UserId,
}

#[derive(DeriveIden)]
enum ChatHistory {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MessageHistory::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(MessageHistory::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(MessageHistory::MessageId).uuid().not_null())
                    .col(
                        ColumnDef::new(MessageHistory::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(MessageHistory::Text).text().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-message_history-message_id")
                            .from(MessageHistory::Table, MessageHistory::MessageId)
                            .to(Messages::Table, Messages::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MessageHistory::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum MessageHistory {
    Table,
    Id,
    MessageId,
    CreatedAt,
    Text,
}

#[derive(DeriveIden)]
enum Messages {
    Table,
    Id,
}

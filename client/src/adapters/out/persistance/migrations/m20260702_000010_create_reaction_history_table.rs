use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ReactionHistory::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ReactionHistory::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ReactionHistory::ReactionId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ReactionHistory::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(ReactionHistory::Text).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-reaction_history-reaction_id")
                            .from(ReactionHistory::Table, ReactionHistory::ReactionId)
                            .to(Reaction::Table, Reaction::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ReactionHistory::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ReactionHistory {
    Table,
    Id,
    ReactionId,
    CreatedAt,
    Text,
}

#[derive(DeriveIden)]
enum Reaction {
    Table,
    Id,
}

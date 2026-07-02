use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserOverlayHistory::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserOverlayHistory::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(UserOverlayHistory::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(UserOverlayHistory::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(UserOverlayHistory::Nickname).string())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-user_overlay_history-user_id")
                            .from(UserOverlayHistory::Table, UserOverlayHistory::UserId)
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
            .drop_table(Table::drop().table(UserOverlayHistory::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum UserOverlayHistory {
    Table,
    Id,
    UserId,
    CreatedAt,
    Nickname,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

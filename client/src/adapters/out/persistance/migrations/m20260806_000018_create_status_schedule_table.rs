use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(StatusSchedule::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(StatusSchedule::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(StatusSchedule::TargetStatus)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StatusSchedule::ScheduledFor)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StatusSchedule::SyncStatus)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StatusSchedule::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StatusSchedule::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-status_schedule-sync_status")
                    .table(StatusSchedule::Table)
                    .col(StatusSchedule::SyncStatus)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(StatusSchedule::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum StatusSchedule {
    Table,
    Id,
    TargetStatus,
    ScheduledFor,
    SyncStatus,
    CreatedAt,
    UpdatedAt,
}

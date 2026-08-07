use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Outbox::Table)
                    .add_column(
                        ColumnDef::new(Outbox::Attempts)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Outbox::Table)
                    .add_column(ColumnDef::new(Outbox::NextRetryAt).timestamp_with_time_zone())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Outbox::Table)
                    .drop_column(Outbox::NextRetryAt)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Outbox::Table)
                    .drop_column(Outbox::Attempts)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Outbox {
    Table,
    Attempts,
    NextRetryAt,
}

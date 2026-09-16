use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(LocalIdentity::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LocalIdentity::DeviceId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(LocalIdentity::UserId).uuid().not_null())
                    .col(ColumnDef::new(LocalIdentity::PublicKey).binary().not_null())
                    .col(
                        ColumnDef::new(LocalIdentity::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(LocalIdentity::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum LocalIdentity {
    Table,
    DeviceId,
    UserId,
    PublicKey,
    CreatedAt,
}

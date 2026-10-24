use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(DeviceKey::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(LocalIdentity::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(DeviceKey::Table)
                    .col(
                        ColumnDef::new(DeviceKey::DeviceId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(DeviceKey::PublicKey).binary().not_null())
                    .col(
                        ColumnDef::new(DeviceKey::EncryptedPrivateKey)
                            .binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(DeviceKey::SigningPublicKey)
                            .binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(DeviceKey::EncryptedSigningPrivateKey)
                            .binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(DeviceKey::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(LocalIdentity::Table)
                    .col(
                        ColumnDef::new(LocalIdentity::DeviceId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(LocalIdentity::UserId).uuid().not_null())
                    .col(ColumnDef::new(LocalIdentity::PublicKey).binary().not_null())
                    .col(
                        ColumnDef::new(LocalIdentity::SigningPublicKey)
                            .binary()
                            .not_null(),
                    )
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
            .drop_table(Table::drop().table(DeviceKey::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(LocalIdentity::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(DeviceKey::Table)
                    .col(
                        ColumnDef::new(DeviceKey::DeviceId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(DeviceKey::PublicKey).binary().not_null())
                    .col(
                        ColumnDef::new(DeviceKey::EncryptedPrivateKey)
                            .binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(DeviceKey::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(LocalIdentity::Table)
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
}

#[derive(DeriveIden)]
enum DeviceKey {
    Table,
    DeviceId,
    PublicKey,
    EncryptedPrivateKey,
    SigningPublicKey,
    EncryptedSigningPrivateKey,
    CreatedAt,
}

#[derive(DeriveIden)]
enum LocalIdentity {
    Table,
    DeviceId,
    UserId,
    PublicKey,
    SigningPublicKey,
    CreatedAt,
}

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PeerDevice::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(PeerDevice::UserId).uuid().not_null())
                    .col(ColumnDef::new(PeerDevice::DeviceId).uuid().not_null())
                    .col(
                        ColumnDef::new(PeerDevice::SigningPublicKey)
                            .binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PeerDevice::AgreementPublicKey)
                            .binary()
                            .not_null(),
                    )
                    .col(ColumnDef::new(PeerDevice::DeviceCert).binary().not_null())
                    .col(
                        ColumnDef::new(PeerDevice::PinnedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(PeerDevice::UserId)
                            .col(PeerDevice::DeviceId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-peer_device-user_id")
                            .from(PeerDevice::Table, PeerDevice::UserId)
                            .to(Users::Table, Users::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(ColumnDef::new(Users::SigningPublicKey).binary())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(ColumnDef::new(Users::VerifiedAt).timestamp_with_time_zone())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::VerifiedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::SigningPublicKey)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(PeerDevice::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PeerDevice {
    Table,
    UserId,
    DeviceId,
    SigningPublicKey,
    AgreementPublicKey,
    DeviceCert,
    PinnedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    SigningPublicKey,
    VerifiedAt,
}

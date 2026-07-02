use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(FileMetadata::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(FileMetadata::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(FileMetadata::FileName).string().not_null())
                    .col(
                        ColumnDef::new(FileMetadata::FileSizeBytes)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(FileMetadata::MimeType).string())
                    .col(ColumnDef::new(FileMetadata::Width).integer())
                    .col(ColumnDef::new(FileMetadata::Height).integer())
                    .col(ColumnDef::new(FileMetadata::DurationSecs).integer())
                    .col(ColumnDef::new(FileMetadata::ThumbnailLink).string())
                    .col(
                        ColumnDef::new(FileMetadata::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FileMetadata::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(FileMetadata::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum FileMetadata {
    Table,
    Id,
    FileName,
    FileSizeBytes,
    MimeType,
    Width,
    Height,
    DurationSecs,
    ThumbnailLink,
    CreatedAt,
    UpdatedAt,
}

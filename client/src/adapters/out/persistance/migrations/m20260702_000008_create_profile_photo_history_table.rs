use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ProfilePhotoHistory::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ProfilePhotoHistory::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ProfilePhotoHistory::UserId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ProfilePhotoHistory::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(ProfilePhotoHistory::AttachmentLink).string())
                    .col(
                        ColumnDef::new(ProfilePhotoHistory::ProfilePhotoType)
                            .tiny_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ProfilePhotoHistory::FileMetadataId)
                            .uuid()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-profile_photo_history-user_id")
                            .from(ProfilePhotoHistory::Table, ProfilePhotoHistory::UserId)
                            .to(User::Table, User::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-profile_photo_history-file_metadata_id")
                            .from(
                                ProfilePhotoHistory::Table,
                                ProfilePhotoHistory::FileMetadataId,
                            )
                            .to(FileMetadata::Table, FileMetadata::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ProfilePhotoHistory::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ProfilePhotoHistory {
    Table,
    Id,
    UserId,
    CreatedAt,
    AttachmentLink,
    ProfilePhotoType,
    FileMetadataId,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum FileMetadata {
    Table,
    Id,
}

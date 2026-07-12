use sea_orm::{ActiveModelTrait, TransactionTrait};
use sea_orm::{DbErr, IntoActiveModel};

use crate::client::adapters::out::persistance::repositories::utils::{
    safe_bulk_insert, safe_bulk_upsert,
};
use crate::client::{
    adapters::out::persistance::{
        entities::{
            attachment, file_metadata, message, message_history, reaction, reaction_history,
        },
        repositories::message::MessageRepository,
    },
    domain::dto::message::{Message, UnstructurizedMessage},
};

pub trait Create {
    async fn create(&self, message: Message) -> Result<(), DbErr>;
    async fn upsert(&self, message: Message) -> Result<(), DbErr>;
}

impl Create for MessageRepository {
    async fn create(&self, message: Message) -> Result<(), DbErr> {
        let (message_model, unstructurized_message_story) = UnstructurizedMessage::from(message);
        let txn = self.db.begin().await?;

        let message_model_active = message_model.into_active_model();
        message_model_active.insert(&txn).await?;

        for (message_story_record, (attachments, file_metadatas), reactions) in
            unstructurized_message_story.iter()
        {
            let message_story_active = message_story_record.clone().into_active_model();
            message_story_active.insert(&txn).await?;

            let attachments_active: Vec<attachment::ActiveModel> = attachments
                .iter()
                .map(|att| att.clone().into_active_model())
                .collect();
            safe_bulk_insert::<attachment::Entity, _, _>(&txn, attachments_active).await?;

            let file_metadatas_active: Vec<file_metadata::ActiveModel> = file_metadatas
                .iter()
                .map(|fm| fm.clone().into_active_model())
                .collect();
            safe_bulk_insert::<file_metadata::Entity, _, _>(&txn, file_metadatas_active).await?;

            for (reaction_model, reaction_history) in reactions {
                let reaction_model_active = reaction_model.clone().into_active_model();
                reaction_model_active.insert(&txn).await?;

                let reaction_stories_active: Vec<reaction_history::ActiveModel> = reaction_history
                    .iter()
                    .map(|rs| rs.clone().into_active_model())
                    .collect();
                safe_bulk_insert::<reaction_history::Entity, _, _>(&txn, reaction_stories_active)
                    .await?;
            }
        }

        txn.commit().await?;
        Ok(())
    }

    async fn upsert(&self, message: Message) -> Result<(), DbErr> {
        let (message_model, unstructurized_message_story) = UnstructurizedMessage::from(message);
        let txn = self.db.begin().await?;

        let message_model_active = message_model.into_active_model();
        safe_bulk_upsert::<message::Entity, _, _>(&txn, vec![message_model_active]).await?;

        for (message_story_record, (attachments, file_metadatas), reactions) in
            unstructurized_message_story.into_iter()
        {
            let message_story_active = message_story_record.into_active_model();
            safe_bulk_upsert::<message_history::Entity, _, _>(&txn, vec![message_story_active])
                .await?;

            let attachments_active: Vec<attachment::ActiveModel> = attachments
                .into_iter()
                .map(|att| att.into_active_model())
                .collect();
            safe_bulk_upsert::<attachment::Entity, _, _>(&txn, attachments_active).await?;

            let file_metadatas_active: Vec<file_metadata::ActiveModel> = file_metadatas
                .into_iter()
                .map(|fm| fm.into_active_model())
                .collect();
            safe_bulk_upsert::<file_metadata::Entity, _, _>(&txn, file_metadatas_active).await?;

            for (reaction_model, reaction_history) in reactions {
                let reaction_model_active = reaction_model.into_active_model();
                safe_bulk_upsert::<reaction::Entity, _, _>(&txn, vec![reaction_model_active])
                    .await?;

                let reaction_stories_active: Vec<reaction_history::ActiveModel> = reaction_history
                    .into_iter()
                    .map(|rs| rs.into_active_model())
                    .collect();
                safe_bulk_upsert::<reaction_history::Entity, _, _>(&txn, reaction_stories_active)
                    .await?;
            }
        }

        txn.commit().await?;
        Ok(())
    }
}

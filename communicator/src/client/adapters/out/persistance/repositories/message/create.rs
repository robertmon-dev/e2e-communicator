use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};
use sea_orm::{DbErr, IntoActiveModel};

use crate::client::{
    adapters::out::persistance::{
        entities::{attachment, file_metadata, message::ActiveModel, reaction, reaction_history},
        repositories::message::MessageRepository,
    },
    domain::dto::message::{Message, UnstructurizedMessage},
};

pub trait Create {
    async fn create(&self, message: Message) -> Result<(), DbErr>;
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

            if !attachments_active.is_empty() {
                attachment::Entity::insert_many(attachments_active)
                    .exec(&txn)
                    .await?;
            }

            let file_metadatas_active: Vec<file_metadata::ActiveModel> = file_metadatas
                .iter()
                .map(|fm| fm.clone().into_active_model())
                .collect();

            if !file_metadatas_active.is_empty() {
                file_metadata::Entity::insert_many(file_metadatas_active)
                    .exec(&txn)
                    .await?;
            }

            for (reaction_model, reaction_history) in reactions {
                let reaction_model_active = reaction_model.clone().into_active_model();
                reaction_model_active.insert(&txn).await?;

                let reaction_stories_active: Vec<reaction_history::ActiveModel> = reaction_history
                    .iter()
                    .map(|rs| rs.clone().into_active_model())
                    .collect();

                if !reaction_stories_active.is_empty() {
                    reaction_history::Entity::insert_many(reaction_stories_active)
                        .exec(&txn)
                        .await?;
                }
            }
        }

        txn.commit().await?;

        Ok(())
    }
}

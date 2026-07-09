use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::IntoParams;
use uuid::Uuid;

use super::utils::{FilterOperator, SortBy, SortDir};

#[derive(Debug, Deserialize, Serialize, Default, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListMessagesFilter {
    pub page: Option<u64>,
    pub per_page: Option<u64>,

    pub chat_id: Option<Uuid>,

    pub action_operator: Option<FilterOperator>,

    pub modified_from: Option<DateTime<Utc>>,
    pub modified_to: Option<DateTime<Utc>>,

    pub sort_by: Option<SortBy>,
    pub sort_dir: Option<SortDir>,

    pub search_text: Option<String>,
    pub search_operator: Option<FilterOperator>,

    pub search_in_latest: bool,
}

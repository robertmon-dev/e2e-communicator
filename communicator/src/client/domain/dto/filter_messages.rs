use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, ExprTrait, sea_query::SimpleExpr};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone, Copy, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, ToSchema)]
pub enum FilterOperator {
    Equals,
    NotEquals,
    In,
    NotIn,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
}

#[derive(Serialize, Deserialize, Debug, ToSchema)]
pub enum SortBy {
    CreatedAt,
    Action,
    Status,
    UserId,
    OrganizationId,
    Provider,
}

#[derive(Debug, Deserialize, Serialize, Default, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListMessagesFilter {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub message_id: Option<Uuid>,
    pub chat_id: Option<Uuid>,
    pub author_id: Option<Uuid>,
    pub action_operator: Option<FilterOperator>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub sort_by: Option<SortBy>,
    pub sort_dir: Option<SortDir>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaginationMeta {
    pub current_page: u64,
    pub per_page: u64,
    pub total_pages: u64,
    pub total_count: u64,
}

pub fn apply_string_filter<T>(
    column: T,
    value: String,
    operator: Option<FilterOperator>,
) -> SimpleExpr
where
    T: ColumnTrait,
{
    let op = operator.unwrap_or(FilterOperator::Equals);

    match op {
        FilterOperator::Equals => column.eq(value),
        FilterOperator::NotEquals => column.ne(value),
        FilterOperator::Contains => column.contains(&value),
        FilterOperator::NotContains => column.contains(&value).not(),
        FilterOperator::StartsWith => column.starts_with(&value),
        FilterOperator::EndsWith => column.ends_with(&value),
        _ => column.eq(value),
    }
}

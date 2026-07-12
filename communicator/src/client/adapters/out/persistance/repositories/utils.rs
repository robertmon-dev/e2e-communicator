use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DbErr, EntityTrait, ExprTrait,
    Iterable, PrimaryKeyToColumn,
    sea_query::{Expr, OnConflict, SimpleExpr},
};

use crate::client::domain::dto::utils::FilterOperator;

pub trait ConditionExt {
    fn add_opt<T, F>(self, opt: Option<T>, f: F) -> Self
    where
        F: FnOnce(T) -> Expr;

    fn add_if(self, condition: bool, expr: Expr) -> Self;
}

impl ConditionExt for Condition {
    fn add_opt<T, F>(self, opt: Option<T>, f: F) -> Self
    where
        F: FnOnce(T) -> Expr,
    {
        if let Some(val) = opt {
            self.add(f(val))
        } else {
            self
        }
    }

    fn add_if(self, condition: bool, expr: Expr) -> Self {
        if condition { self.add(expr) } else { self }
    }
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

pub async fn safe_bulk_insert<E, A, C>(db: &C, models: Vec<A>) -> Result<(), DbErr>
where
    E: EntityTrait,
    A: ActiveModelTrait<Entity = E> + Send + Sync,
    C: ConnectionTrait,
{
    if !models.is_empty() {
        E::insert_many(models).exec(db).await?;
    }
    Ok(())
}

pub async fn safe_bulk_upsert<E, A, C>(db: &C, models: Vec<A>) -> Result<(), DbErr>
where
    E: EntityTrait,
    A: ActiveModelTrait<Entity = E> + Send + Sync,
    C: ConnectionTrait,
{
    if !models.is_empty() {
        let on_conflict = OnConflict::columns(E::PrimaryKey::iter().map(|pk| pk.into_column()))
            .update_columns(E::Column::iter())
            .to_owned();

        E::insert_many(models)
            .on_conflict(on_conflict)
            .exec(db)
            .await?;
    }
    Ok(())
}

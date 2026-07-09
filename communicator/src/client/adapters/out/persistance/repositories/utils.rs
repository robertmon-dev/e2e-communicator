use crate::client::domain::dto::utils::FilterOperator;
use sea_orm::{ColumnTrait, Condition, ExprTrait, sea_query::SimpleExpr};

pub trait ConditionExt {
    fn add_opt<T, F>(self, opt: Option<T>, f: F) -> Self
    where
        F: FnOnce(T) -> SimpleExpr;

    fn add_if(self, condition: bool, expr: SimpleExpr) -> Self;
}

impl ConditionExt for Condition {
    fn add_opt<T, F>(self, opt: Option<T>, f: F) -> Self
    where
        F: FnOnce(T) -> SimpleExpr,
    {
        if let Some(val) = opt {
            self.add(f(val))
        } else {
            self
        }
    }

    fn add_if(self, condition: bool, expr: SimpleExpr) -> Self {
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

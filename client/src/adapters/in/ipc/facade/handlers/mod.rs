use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::adapters::r#in::ipc::protocol::Outcome;

use super::Facade;
use super::utils::{IntoOutcome, ok, parse};

pub trait RpcHandler: for<'de> Deserialize<'de> {
    type Output: Serialize;
    type Error: IntoOutcome;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error>;
}

pub async fn execute_rpc<H: RpcHandler>(facade: &Facade, params: Value) -> Outcome {
    let parsed: H = match parse(params) {
        Ok(p) => p,
        Err(e) => return e,
    };

    match parsed.handle(facade).await {
        Ok(val) => ok(serde_json::to_value(val).expect("Serialization failed")),
        Err(err) => err.into_outcome(),
    }
}

pub mod attachments;
pub mod chats;
pub mod identity;
pub mod invitations;
pub mod messages;
pub mod users;

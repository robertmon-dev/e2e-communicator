use chrono::Duration;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeStep {
    Sec10,
    Sec20,
    Sec30,
    Sec45,
    Min1,
}

impl TimeStep {
    pub fn as_duration(&self) -> Duration {
        match self {
            Self::Sec10 => Duration::seconds(10),
            Self::Sec20 => Duration::seconds(20),
            Self::Sec30 => Duration::seconds(30),
            Self::Sec45 => Duration::seconds(45),
            Self::Min1 => Duration::seconds(60),
        }
    }

    pub fn as_secs(&self) -> u64 {
        match self {
            Self::Sec10 => 10,
            Self::Sec20 => 20,
            Self::Sec30 => 30,
            Self::Sec45 => 45,
            Self::Min1 => 60,
        }
    }
}

use crate::core::initializers::settings::{AppEnv, Settings};

pub fn init(settings: &Settings) {
    common::logging::init(
        &settings.logging.logging_level,
        settings.environment == AppEnv::Production,
    );
}

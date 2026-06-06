use std::env;

use anyhow::{bail, Result};

#[derive(Clone)]
pub struct Config {
    pub discord_token: String,
    pub database_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let discord_token = env::var("DISCORD_TOKEN").unwrap_or_default();
        if discord_token.trim().is_empty() {
            bail!("DISCORD_TOKEN is required");
        }

        Ok(Self {
            discord_token,
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://dictator-bot.db?mode=rwc".to_string()),
        })
    }
}

mod ai;
mod commands;
mod config;
mod cve;
mod db;
mod os_updates;
mod scheduler;
mod snippets;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::Context as _;
use serenity::all::{
    async_trait, Client, Context, EventHandler, GatewayIntents, Interaction, Message, Ready,
};
use tracing::{error, info};

use crate::{config::Config, db::Db};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub db: Db,
    pub http: reqwest::Client,
    scheduler_started: Arc<AtomicBool>,
}

impl AppState {
    fn new(config: Config, db: Db) -> Self {
        Self {
            config,
            db,
            http: reqwest::Client::builder()
                .user_agent("dictator-bot/0.1")
                .build()
                .expect("reqwest client"),
            scheduler_started: Arc::new(AtomicBool::new(false)),
        }
    }
}

struct Handler {
    state: AppState,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("logged in as {}", ready.user.name);

        if let Err(err) = commands::register_global_commands(&ctx).await {
            error!("failed to register slash commands: {err:?}");
        }

        if !self.state.scheduler_started.swap(true, Ordering::SeqCst) {
            scheduler::spawn(ctx, self.state.clone());
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Err(err) = commands::handle_interaction(&ctx, &self.state, interaction).await {
            error!("interaction error: {err:?}");
        }
    }

    async fn message(&self, ctx: Context, msg: Message) {
        if let Err(err) = snippets::handle_message(&ctx, &self.state, &msg).await {
            error!("snippet message error: {err:?}");
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env()?;
    let db = Db::connect(&config.database_url).await?;
    db.migrate().await?;

    let intents =
        GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&config.discord_token, intents)
        .event_handler(Handler {
            state: AppState::new(config, db),
        })
        .await
        .context("failed to build Discord client")?;

    client.start().await.context("Discord client stopped")?;
    Ok(())
}

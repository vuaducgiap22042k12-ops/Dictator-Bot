use anyhow::Result;
use chrono::{Datelike, Duration as ChronoDuration, Timelike, Utc};
use serenity::all::{ChannelId, Context, CreateEmbed, CreateEmbedFooter, CreateMessage};
use tokio::time::{sleep, Duration};
use tracing::error;

use crate::{commands, db::SeenItem, AppState};

const CVE_CHANNEL_KEY: &str = "cve_channel";
const UPDATE_CHANNEL_KEY: &str = "update_channel";

pub fn spawn(ctx: Context, state: AppState) {
    spawn_cve_loop(ctx.clone(), state.clone());
    spawn_update_loop(ctx.clone(), state.clone());
    spawn_daily_summary_loop(ctx, state);
}

fn spawn_cve_loop(ctx: Context, state: AppState) {
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(30 * 60)).await;
            if let Err(err) = run_for_configured_channels(&ctx, &state, CVE_CHANNEL_KEY, true).await
            {
                error!("scheduled CVE check failed: {err:?}");
            }
        }
    });
}

fn spawn_update_loop(ctx: Context, state: AppState) {
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(60 * 60)).await;
            if let Err(err) =
                run_for_configured_channels(&ctx, &state, UPDATE_CHANNEL_KEY, false).await
            {
                error!("scheduled OS update check failed: {err:?}");
            }
        }
    });
}

async fn run_for_configured_channels(
    ctx: &Context,
    state: &AppState,
    key: &str,
    cves: bool,
) -> Result<()> {
    for (_, channel) in state.db.settings_by_key(key).await? {
        let Ok(channel_id) = channel.parse::<u64>() else {
            continue;
        };

        if cves {
            commands::run_cve_check(ctx, state, channel_id).await?;
        } else {
            commands::run_update_check(ctx, state, channel_id).await?;
        }
    }

    Ok(())
}

fn spawn_daily_summary_loop(ctx: Context, state: AppState) {
    tokio::spawn(async move {
        let mut last_sent_key = String::new();

        loop {
            sleep(Duration::from_secs(60)).await;
            let now = Utc::now();
            if now.hour() != 0 || now.minute() != 0 {
                continue;
            }

            let summary_date = (now.date_naive() - ChronoDuration::days(1)).to_string();
            let key = format!("{}-{}-{}", now.year(), now.ordinal(), summary_date);
            if key == last_sent_key {
                continue;
            }

            if let Err(err) = send_daily_summaries(&ctx, &state, &summary_date).await {
                error!("daily summary failed: {err:?}");
            } else {
                last_sent_key = key;
            }
        }
    });
}

async fn send_daily_summaries(ctx: &Context, state: &AppState, date: &str) -> Result<()> {
    let cves = state.db.seen_for_utc_date("cve", date).await?;
    for (_, channel) in state.db.settings_by_key(CVE_CHANNEL_KEY).await? {
        let Ok(channel_id) = channel.parse::<u64>() else {
            continue;
        };
        send_summary(ctx, channel_id, "Daily CVE Summary", date, &cves, 0xd64045).await?;
    }

    let releases = state.db.seen_for_utc_date("os_release", date).await?;
    for (_, channel) in state.db.settings_by_key(UPDATE_CHANNEL_KEY).await? {
        let Ok(channel_id) = channel.parse::<u64>() else {
            continue;
        };
        send_summary(
            ctx,
            channel_id,
            "Daily OS Release Summary",
            date,
            &releases,
            0x2d7dd2,
        )
        .await?;
    }

    Ok(())
}

async fn send_summary(
    ctx: &Context,
    channel_id: u64,
    title: &str,
    date: &str,
    items: &[SeenItem],
    color: u32,
) -> Result<()> {
    let mut embed = CreateEmbed::new()
        .title(title)
        .description(format!("UTC date: {date}"))
        .color(color)
        .footer(CreateEmbedFooter::new(format!("{} item(s)", items.len())));

    if items.is_empty() {
        embed = embed.field("No updates", "No matching items were seen.", false);
    } else {
        for item in items.iter().take(20) {
            embed = embed.field(
                format!("{} | {}", item.os_family, item.external_id),
                format!(
                    "[link]({}) | {}\n{}",
                    item.url, item.published_at, item.title
                ),
                false,
            );
        }

        if items.len() > 20 {
            embed = embed.field("More", format!("{} more item(s).", items.len() - 20), false);
        }
    }

    ChannelId::new(channel_id)
        .send_message(&ctx.http, CreateMessage::new().embed(embed))
        .await?;

    Ok(())
}

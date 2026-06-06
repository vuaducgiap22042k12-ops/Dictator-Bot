use anyhow::Result;
use serenity::all::{Context, CreateMessage, Message};

use crate::AppState;

pub async fn handle_message(ctx: &Context, state: &AppState, msg: &Message) -> Result<()> {
    if msg.author.bot {
        return Ok(());
    }

    let Some(guild_id) = msg.guild_id else {
        return Ok(());
    };

    let content = msg.content.trim();
    let Some(input) = snippet_input(content) else {
        return Ok(());
    };

    let mut parts = input.splitn(3, char::is_whitespace);
    let command = parts.next().unwrap_or_default().to_string();
    let first = parts.next().unwrap_or_default().trim().to_string();
    let rest = parts.next().unwrap_or_default().trim().to_string();
    let is_admin = msg
        .member
        .as_ref()
        .and_then(|member| member.permissions)
        .map(|permissions| permissions.administrator())
        .unwrap_or(false);

    match command.as_str() {
        "ls" => list_snippets(ctx, state, msg, guild_id.get()).await?,
        "a" => create_alias(ctx, state, msg, guild_id.get(), &first, &rest).await?,
        "si" => snippet_info(ctx, state, msg, guild_id.get(), &first).await?,
        "sc" => create_snippet(ctx, state, msg, guild_id.get(), &first, &rest).await?,
        "se" => edit_snippet(ctx, state, msg, guild_id.get(), &first, &rest, is_admin).await?,
        "sl" => toggle_lock(ctx, state, msg, guild_id.get(), &first).await?,
        "sd" => delete_snippet(ctx, state, msg, guild_id.get(), &first, is_admin).await?,
        name if !name.is_empty() && first.is_empty() && rest.is_empty() => {
            view_snippet(ctx, state, msg, guild_id.get(), name).await?
        }
        _ => {}
    }

    Ok(())
}

fn snippet_input(content: &str) -> Option<String> {
    if let Some(stripped) = content.strip_prefix('/') {
        return Some(stripped.trim().to_string());
    }

    content
        .strip_prefix("sd ")
        .map(|name| format_sd_input(name))
}

fn format_sd_input(name: &str) -> String {
    format!("sd {}", name.trim())
}

async fn view_snippet(
    ctx: &Context,
    state: &AppState,
    msg: &Message,
    guild_id: u64,
    raw_name: &str,
) -> Result<()> {
    let Some(name) = normalize_name(raw_name) else {
        return Ok(());
    };

    match state.db.resolve_snippet(guild_id, &name).await? {
        Some(snippet) => reply(ctx, msg, clamp(&snippet.content, 1900)).await?,
        None => {}
    }

    Ok(())
}

async fn list_snippets(
    ctx: &Context,
    state: &AppState,
    msg: &Message,
    guild_id: u64,
) -> Result<()> {
    let snippets = state.db.list_snippets(guild_id).await?;
    if snippets.is_empty() {
        return reply(ctx, msg, "No snippets.").await;
    }

    let names = snippets
        .into_iter()
        .map(|snippet| {
            if snippet.locked {
                format!("{} [locked]", snippet.name)
            } else {
                snippet.name
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    reply(ctx, msg, clamp(&names, 1900)).await
}

async fn create_alias(
    ctx: &Context,
    state: &AppState,
    msg: &Message,
    guild_id: u64,
    raw_target: &str,
    raw_alias: &str,
) -> Result<()> {
    let Some(target) = normalize_name(raw_target) else {
        return reply(ctx, msg, "Usage: /a [target] [alias]").await;
    };
    let Some(alias) = normalize_name(raw_alias) else {
        return reply(ctx, msg, "Usage: /a [target] [alias]").await;
    };

    let created = state
        .db
        .create_alias(guild_id, &target, &alias, msg.author.id.get())
        .await?;

    if created {
        reply(ctx, msg, format!("Alias `{alias}` -> `{target}` created.")).await
    } else {
        reply(ctx, msg, "Target missing or alias already exists.").await
    }
}

async fn snippet_info(
    ctx: &Context,
    state: &AppState,
    msg: &Message,
    guild_id: u64,
    raw_name: &str,
) -> Result<()> {
    let Some(name) = normalize_name(raw_name) else {
        return reply(ctx, msg, "Usage: /si [name]").await;
    };

    match state.db.snippet(guild_id, &name).await? {
        Some(snippet) => {
            reply(
                ctx,
                msg,
                format!(
                    "`{}`\nowner: <@{}>\nlocked: {}\ncreated: {}\nupdated: {}",
                    snippet.name,
                    snippet.owner_id,
                    snippet.locked,
                    snippet.created_at,
                    snippet.updated_at
                ),
            )
            .await
        }
        None => reply(ctx, msg, "Snippet not found.").await,
    }
}

async fn create_snippet(
    ctx: &Context,
    state: &AppState,
    msg: &Message,
    guild_id: u64,
    raw_name: &str,
    content: &str,
) -> Result<()> {
    let Some(name) = normalize_name(raw_name) else {
        return reply(ctx, msg, "Usage: /sc [name] [content]").await;
    };
    if content.is_empty() {
        return reply(ctx, msg, "Usage: /sc [name] [content]").await;
    }

    let created = state
        .db
        .create_snippet(guild_id, &name, content, msg.author.id.get())
        .await?;

    if created {
        reply(ctx, msg, format!("Snippet `{name}` created.")).await
    } else {
        reply(ctx, msg, "Snippet already exists.").await
    }
}

async fn edit_snippet(
    ctx: &Context,
    state: &AppState,
    msg: &Message,
    guild_id: u64,
    raw_name: &str,
    content: &str,
    is_admin: bool,
) -> Result<()> {
    let Some(name) = normalize_name(raw_name) else {
        return reply(ctx, msg, "Usage: /se [name] [content]").await;
    };
    if content.is_empty() {
        return reply(ctx, msg, "Usage: /se [name] [content]").await;
    }

    let updated = state
        .db
        .update_snippet(guild_id, &name, content, msg.author.id.get(), is_admin)
        .await?;

    if updated {
        reply(ctx, msg, format!("Snippet `{name}` updated.")).await
    } else {
        reply(ctx, msg, "Snippet not found or locked.").await
    }
}

async fn toggle_lock(
    ctx: &Context,
    state: &AppState,
    msg: &Message,
    guild_id: u64,
    raw_name: &str,
) -> Result<()> {
    let Some(name) = normalize_name(raw_name) else {
        return reply(ctx, msg, "Usage: /sl [name]").await;
    };

    match state
        .db
        .toggle_lock(guild_id, &name, msg.author.id.get())
        .await?
    {
        Some(true) => reply(ctx, msg, format!("Snippet `{name}` locked.")).await,
        Some(false) => reply(ctx, msg, format!("Snippet `{name}` unlocked.")).await,
        None => reply(ctx, msg, "Snippet not found or you are not the owner.").await,
    }
}

async fn delete_snippet(
    ctx: &Context,
    state: &AppState,
    msg: &Message,
    guild_id: u64,
    raw_name: &str,
    is_admin: bool,
) -> Result<()> {
    let Some(name) = normalize_name(raw_name) else {
        return reply(ctx, msg, "Usage: /sd [name]").await;
    };

    let deleted = state
        .db
        .delete_snippet(guild_id, &name, msg.author.id.get(), is_admin)
        .await?;

    if deleted {
        reply(ctx, msg, format!("Snippet `{name}` deleted.")).await
    } else {
        reply(ctx, msg, "Snippet not found or no permission.").await
    }
}

async fn reply(ctx: &Context, msg: &Message, content: impl Into<String>) -> Result<()> {
    msg.channel_id
        .send_message(
            &ctx.http,
            CreateMessage::new()
                .content(content.into())
                .reference_message(msg),
        )
        .await?;
    Ok(())
}

fn normalize_name(value: &str) -> Option<String> {
    let name = value.trim().trim_start_matches('/').to_ascii_lowercase();
    if name.is_empty() || name.len() > 64 {
        return None;
    }

    if name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        Some(name)
    } else {
        None
    }
}

fn clamp(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }

    let mut output = value
        .chars()
        .take(max.saturating_sub(3))
        .collect::<String>();
    output.push_str("...");
    output
}

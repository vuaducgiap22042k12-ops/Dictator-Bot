use anyhow::{anyhow, Result};
use serenity::all::{
    ChannelId, Command, CommandDataOption, CommandDataOptionValue, CommandInteraction,
    CommandOptionType, Context, CreateCommand, CreateCommandOption, CreateEmbed,
    CreateInteractionResponse, CreateInteractionResponseMessage, EditInteractionResponse,
    Interaction,
};

use crate::{ai, cve, db::SeenItem, os_updates, AppState};

const CVE_CHANNEL_KEY: &str = "cve_channel";
const UPDATE_CHANNEL_KEY: &str = "update_channel";

pub async fn register_global_commands(ctx: &Context) -> Result<()> {
    let setup = CreateCommand::new("setup")
        .description("Configure bot channels")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "cve",
                "Set CVE update channel",
            )
            .add_sub_option(
                CreateCommandOption::new(CommandOptionType::Channel, "channel", "CVE channel")
                    .required(true),
            ),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "update",
                "Set OS release update channel",
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::Channel,
                    "channel",
                    "OS update channel",
                )
                .required(true),
            ),
        );

    let cves = CreateCommand::new("cves")
        .description("CVE commands")
        .add_option(CreateCommandOption::new(
            CommandOptionType::SubCommand,
            "now",
            "Check CVEs now",
        ))
        .add_option(CreateCommandOption::new(
            CommandOptionType::SubCommand,
            "ls",
            "List recent seen CVEs",
        ));

    let update = CreateCommand::new("update")
        .description("OS release update commands")
        .add_option(CreateCommandOption::new(
            CommandOptionType::SubCommand,
            "now",
            "Check OS releases now",
        ));

    let snippet = CreateCommand::new("snippet")
        .description("Snippet commands")
        .add_option(
            CreateCommandOption::new(CommandOptionType::SubCommand, "view", "View a snippet")
                .add_sub_option(
                    CreateCommandOption::new(CommandOptionType::String, "name", "Snippet name")
                        .required(true),
                ),
        )
        .add_option(CreateCommandOption::new(
            CommandOptionType::SubCommand,
            "list",
            "List snippets",
        ))
        .add_option(
            CreateCommandOption::new(CommandOptionType::SubCommand, "create", "Create a snippet")
                .add_sub_option(
                    CreateCommandOption::new(CommandOptionType::String, "name", "Snippet name")
                        .required(true),
                )
                .add_sub_option(
                    CreateCommandOption::new(
                        CommandOptionType::String,
                        "content",
                        "Snippet content",
                    )
                    .required(true),
                ),
        )
        .add_option(
            CreateCommandOption::new(CommandOptionType::SubCommand, "edit", "Edit a snippet")
                .add_sub_option(
                    CreateCommandOption::new(CommandOptionType::String, "name", "Snippet name")
                        .required(true),
                )
                .add_sub_option(
                    CreateCommandOption::new(CommandOptionType::String, "content", "New content")
                        .required(true),
                ),
        )
        .add_option(
            CreateCommandOption::new(CommandOptionType::SubCommand, "info", "Show snippet info")
                .add_sub_option(
                    CreateCommandOption::new(CommandOptionType::String, "name", "Snippet name")
                        .required(true),
                ),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "alias",
                "Create a snippet alias",
            )
            .add_sub_option(
                CreateCommandOption::new(CommandOptionType::String, "target", "Target snippet")
                    .required(true),
            )
            .add_sub_option(
                CreateCommandOption::new(CommandOptionType::String, "alias", "Alias name")
                    .required(true),
            ),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "lock",
                "Lock or unlock a snippet",
            )
            .add_sub_option(
                CreateCommandOption::new(CommandOptionType::String, "name", "Snippet name")
                    .required(true),
            ),
        )
        .add_option(
            CreateCommandOption::new(CommandOptionType::SubCommand, "delete", "Delete a snippet")
                .add_sub_option(
                    CreateCommandOption::new(CommandOptionType::String, "name", "Snippet name")
                        .required(true),
                ),
        );

    let ask = CreateCommand::new("ask").description("Open a temporary AI quick ask panel");

    Command::set_global_commands(&ctx.http, vec![setup, cves, update, snippet, ask]).await?;
    Ok(())
}

pub async fn handle_interaction(
    ctx: &Context,
    state: &AppState,
    interaction: Interaction,
) -> Result<()> {
    let Interaction::Command(command) = interaction else {
        return Ok(());
    };

    match command.data.name.as_str() {
        "setup" => handle_setup(ctx, state, &command).await,
        "cves" => handle_cves(ctx, state, &command).await,
        "update" => handle_update(ctx, state, &command).await,
        "snippet" => handle_snippet(ctx, state, &command).await,
        "ask" => ai::send_quick_panel(ctx, &command).await,
        _ => Ok(()),
    }
}

pub async fn run_cve_check(ctx: &Context, state: &AppState, channel_id: u64) -> Result<usize> {
    let items = cve::fetch_recent(&state.http, 48).await?;
    let fresh = cve::store_new(&state.db, &items).await?;
    cve::send_cves(ctx, channel_id, &fresh).await?;
    Ok(fresh.len())
}

pub async fn run_update_check(ctx: &Context, state: &AppState, channel_id: u64) -> Result<usize> {
    let items = os_updates::fetch_releases(&state.http).await?;
    let fresh = os_updates::store_new(&state.db, &items).await?;
    os_updates::send_releases(ctx, channel_id, &fresh).await?;
    Ok(fresh.len())
}

async fn handle_setup(ctx: &Context, state: &AppState, command: &CommandInteraction) -> Result<()> {
    let guild_id = command
        .guild_id
        .ok_or_else(|| anyhow!("setup must run in a guild"))?;
    let (subcommand, options) = subcommand(&command.data.options)?;
    let channel_id = channel_option(options, "channel")?;

    match subcommand {
        "cve" => {
            state
                .db
                .set_setting(
                    guild_id.get(),
                    CVE_CHANNEL_KEY,
                    &channel_id.get().to_string(),
                )
                .await?;
            ephemeral(
                ctx,
                command,
                format!("CVE channel set to <#{}>.", channel_id.get()),
            )
            .await?;
        }
        "update" => {
            state
                .db
                .set_setting(
                    guild_id.get(),
                    UPDATE_CHANNEL_KEY,
                    &channel_id.get().to_string(),
                )
                .await?;
            ephemeral(
                ctx,
                command,
                format!("OS update channel set to <#{}>.", channel_id.get()),
            )
            .await?;
        }
        _ => ephemeral(ctx, command, "Unknown setup target.").await?,
    }

    Ok(())
}

async fn handle_cves(ctx: &Context, state: &AppState, command: &CommandInteraction) -> Result<()> {
    let guild_id = command
        .guild_id
        .ok_or_else(|| anyhow!("cves must run in a guild"))?;
    let (subcommand, _) = subcommand(&command.data.options)?;

    match subcommand {
        "now" => {
            defer_ephemeral(ctx, command).await?;
            let channel = configured_channel(state, guild_id.get(), CVE_CHANNEL_KEY)
                .await?
                .unwrap_or_else(|| command.channel_id.get());
            let count = run_cve_check(ctx, state, channel).await?;
            edit_response(
                ctx,
                command,
                if count == 0 {
                    "No new Linux/Windows/macOS/BSD CVEs.".to_string()
                } else {
                    format!("Sent {count} new CVE(s) to <#{channel}>.")
                },
            )
            .await?;
        }
        "ls" => {
            let seen = state.db.recent_seen("cve", 25).await?;
            send_seen_list(ctx, command, "Recent Seen CVEs", seen).await?;
        }
        _ => ephemeral(ctx, command, "Unknown CVE command.").await?,
    }

    Ok(())
}

async fn handle_update(
    ctx: &Context,
    state: &AppState,
    command: &CommandInteraction,
) -> Result<()> {
    let guild_id = command
        .guild_id
        .ok_or_else(|| anyhow!("update must run in a guild"))?;
    let (subcommand, _) = subcommand(&command.data.options)?;

    match subcommand {
        "now" => {
            defer_ephemeral(ctx, command).await?;
            let channel = configured_channel(state, guild_id.get(), UPDATE_CHANNEL_KEY)
                .await?
                .unwrap_or_else(|| command.channel_id.get());
            let count = run_update_check(ctx, state, channel).await?;
            edit_response(
                ctx,
                command,
                if count == 0 {
                    "No new OS release updates.".to_string()
                } else {
                    format!("Sent {count} OS release update(s) to <#{channel}>.")
                },
            )
            .await?;
        }
        _ => ephemeral(ctx, command, "Unknown update command.").await?,
    }

    Ok(())
}

async fn handle_snippet(
    ctx: &Context,
    state: &AppState,
    command: &CommandInteraction,
) -> Result<()> {
    let guild_id = command
        .guild_id
        .ok_or_else(|| anyhow!("snippet must run in a guild"))?
        .get();
    let (subcommand, options) = subcommand(&command.data.options)?;
    let user_id = command.user.id.get();
    let is_admin = command
        .member
        .as_ref()
        .and_then(|member| member.permissions)
        .map(|permissions| permissions.administrator())
        .unwrap_or(false);

    match subcommand {
        "view" => {
            let name = normalized_string_option(options, "name")?;
            match state.db.resolve_snippet(guild_id, &name).await? {
                Some(snippet) => ephemeral(ctx, command, clamp(&snippet.content, 1900)).await?,
                None => ephemeral(ctx, command, "Snippet not found.").await?,
            }
        }
        "list" => {
            let snippets = state.db.list_snippets(guild_id).await?;
            if snippets.is_empty() {
                ephemeral(ctx, command, "No snippets.").await?;
            } else {
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
                ephemeral(ctx, command, clamp(&names, 1900)).await?;
            }
        }
        "create" => {
            let name = normalized_string_option(options, "name")?;
            let content = string_option(options, "content")?;
            let created = state
                .db
                .create_snippet(guild_id, &name, content, user_id)
                .await?;
            if created {
                ephemeral(ctx, command, format!("Snippet `{name}` created.")).await?;
            } else {
                ephemeral(ctx, command, "Snippet already exists.").await?;
            }
        }
        "edit" => {
            let name = normalized_string_option(options, "name")?;
            let content = string_option(options, "content")?;
            let updated = state
                .db
                .update_snippet(guild_id, &name, content, user_id, is_admin)
                .await?;
            if updated {
                ephemeral(ctx, command, format!("Snippet `{name}` updated.")).await?;
            } else {
                ephemeral(ctx, command, "Snippet not found or locked.").await?;
            }
        }
        "info" => {
            let name = normalized_string_option(options, "name")?;
            match state.db.snippet(guild_id, &name).await? {
                Some(snippet) => {
                    ephemeral(
                        ctx,
                        command,
                        format!(
                            "`{}`\nowner: <@{}>\nlocked: {}\ncreated: {}\nupdated: {}",
                            snippet.name,
                            snippet.owner_id,
                            snippet.locked,
                            snippet.created_at,
                            snippet.updated_at
                        ),
                    )
                    .await?;
                }
                None => ephemeral(ctx, command, "Snippet not found.").await?,
            }
        }
        "alias" => {
            let target = normalized_string_option(options, "target")?;
            let alias = normalized_string_option(options, "alias")?;
            let created = state
                .db
                .create_alias(guild_id, &target, &alias, user_id)
                .await?;
            if created {
                ephemeral(
                    ctx,
                    command,
                    format!("Alias `{alias}` -> `{target}` created."),
                )
                .await?;
            } else {
                ephemeral(ctx, command, "Target missing or alias already exists.").await?;
            }
        }
        "lock" => {
            let name = normalized_string_option(options, "name")?;
            match state.db.toggle_lock(guild_id, &name, user_id).await? {
                Some(true) => ephemeral(ctx, command, format!("Snippet `{name}` locked.")).await?,
                Some(false) => {
                    ephemeral(ctx, command, format!("Snippet `{name}` unlocked.")).await?
                }
                None => {
                    ephemeral(ctx, command, "Snippet not found or you are not the owner.").await?
                }
            }
        }
        "delete" => {
            let name = normalized_string_option(options, "name")?;
            let deleted = state
                .db
                .delete_snippet(guild_id, &name, user_id, is_admin)
                .await?;
            if deleted {
                ephemeral(ctx, command, format!("Snippet `{name}` deleted.")).await?;
            } else {
                ephemeral(ctx, command, "Snippet not found or no permission.").await?;
            }
        }
        _ => ephemeral(ctx, command, "Unknown snippet command.").await?,
    }

    Ok(())
}

async fn configured_channel(state: &AppState, guild_id: u64, key: &str) -> Result<Option<u64>> {
    Ok(state
        .db
        .setting(guild_id, key)
        .await?
        .and_then(|value| value.parse::<u64>().ok()))
}

async fn send_seen_list(
    ctx: &Context,
    command: &CommandInteraction,
    title: &str,
    items: Vec<SeenItem>,
) -> Result<()> {
    if items.is_empty() {
        return ephemeral(ctx, command, "No seen CVEs yet.").await;
    }

    let mut embed = CreateEmbed::new().title(title).color(0x5865f2);
    for item in items {
        embed = embed.field(
            item.external_id,
            format!(
                "[NVD]({}) | {} | published: {} | seen: {}\n{}",
                item.url, item.os_family, item.published_at, item.first_seen_at, item.title
            ),
            false,
        );
    }

    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .ephemeral(true),
            ),
        )
        .await?;
    Ok(())
}

async fn ephemeral(
    ctx: &Context,
    command: &CommandInteraction,
    content: impl Into<String>,
) -> Result<()> {
    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content.into())
                    .ephemeral(true),
            ),
        )
        .await?;
    Ok(())
}

async fn defer_ephemeral(ctx: &Context, command: &CommandInteraction) -> Result<()> {
    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Defer(
                CreateInteractionResponseMessage::new().ephemeral(true),
            ),
        )
        .await?;
    Ok(())
}

async fn edit_response(ctx: &Context, command: &CommandInteraction, content: String) -> Result<()> {
    command
        .edit_response(&ctx.http, EditInteractionResponse::new().content(content))
        .await?;
    Ok(())
}

fn subcommand(options: &[CommandDataOption]) -> Result<(&str, &[CommandDataOption])> {
    let option = options
        .first()
        .ok_or_else(|| anyhow!("missing subcommand"))?;
    match &option.value {
        CommandDataOptionValue::SubCommand(options) => {
            Ok((option.name.as_str(), options.as_slice()))
        }
        _ => Err(anyhow!("expected subcommand")),
    }
}

fn channel_option(options: &[CommandDataOption], name: &str) -> Result<ChannelId> {
    let option = options
        .iter()
        .find(|option| option.name == name)
        .ok_or_else(|| anyhow!("missing channel option"))?;

    match &option.value {
        CommandDataOptionValue::Channel(channel_id) => Ok(*channel_id),
        _ => Err(anyhow!("expected channel option")),
    }
}

fn string_option<'a>(options: &'a [CommandDataOption], name: &str) -> Result<&'a str> {
    let option = options
        .iter()
        .find(|option| option.name == name)
        .ok_or_else(|| anyhow!("missing string option"))?;

    match &option.value {
        CommandDataOptionValue::String(value) => Ok(value.as_str()),
        _ => Err(anyhow!("expected string option")),
    }
}

fn normalized_string_option(options: &[CommandDataOption], name: &str) -> Result<String> {
    normalize_name(string_option(options, name)?).ok_or_else(|| anyhow!("invalid snippet name"))
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

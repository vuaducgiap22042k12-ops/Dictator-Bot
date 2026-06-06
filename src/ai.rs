use anyhow::Result;
use serenity::all::{
    CommandInteraction, Context, CreateActionRow, CreateButton, CreateEmbed,
    CreateInteractionResponse, CreateInteractionResponseMessage,
};

pub async fn send_quick_panel(ctx: &Context, command: &CommandInteraction) -> Result<()> {
    let embed = CreateEmbed::new()
        .title("AI Quick Ask")
        .description("Open an AI chat site and ask directly on the web.")
        .color(0x10a37f);

    let components = vec![CreateActionRow::Buttons(vec![
        CreateButton::new_link("https://chatgpt.com/").label("ChatGPT"),
        CreateButton::new_link("https://gemini.google.com/").label("Gemini"),
        CreateButton::new_link("https://www.perplexity.ai/").label("Perplexity"),
    ])];

    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .components(components),
            ),
        )
        .await?;

    let http = ctx.http.clone();
    let command = command.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        let _ = command.delete_response(&http).await;
    });

    Ok(())
}

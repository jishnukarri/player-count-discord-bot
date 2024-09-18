use serenity::model::prelude::command::CommandOptionType;
use serenity::model::prelude::{CommandInteraction, InteractionResponseType};

// Add this to your Handler to listen for slash commands
#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        // Register a slash command for player info
        let _ = Command::create_global_application_command(&ctx.http, |command| {
            command.name("playerlist").description("Get detailed player info")
        })
        .await;
        
        tokio::spawn(server_activity(ctx));
    }

    async fn interaction_create(&self, ctx: Context, interaction: CommandInteraction) {
        if interaction.data.name == "playerlist" {
            let guard = ctx.data.read().await;
            let addr = guard.get::<TMAddress>().unwrap();
            let a2s = A2SClient::new().await.unwrap();

            // Fetch player info
            match a2s.players(addr).await {
                Ok(players) => {
                    let mut player_info = String::from("```\n Player   |   Score | Time\n----------+---------+------------\n");

                    for player in players {
                        let time_played = format!("{}h {}m {}s", player.duration.as_secs() / 3600, (player.duration.as_secs() % 3600) / 60, player.duration.as_secs() % 60);
                        player_info.push_str(&format!("{:<9} | {:>7} | {}\n", player.name, player.score, time_played));
                    }

                    player_info.push_str("```");

                    // Respond with the detailed player table
                    let _ = interaction.create_interaction_response(&ctx.http, |response| {
                        response
                            .kind(InteractionResponseType::ChannelMessageWithSource)
                            .interaction_response_data(|message| message.content(player_info))
                    })
                    .await;
                }
                Err(_) => {
                    let _ = interaction.create_interaction_response(&ctx.http, |response| {
                        response
                            .kind(InteractionResponseType::ChannelMessageWithSource)
                            .interaction_response_data(|message| message.content("Failed to get player information."))
                    })
                    .await;
                }
            }
        }
    }
}

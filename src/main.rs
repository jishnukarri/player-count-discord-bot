use serenity::all::{ActivityData, Context, EventHandler, GatewayIntents, Interaction, InteractionResponseFlags, Command};
use serenity::async_trait;
use serenity::prelude::TypeMapKey;
use serenity::utils::token::validate;
use serenity::Client;

use a2s::{self, A2SClient};

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::default::Default;
use std::fs;
use std::io::{self, Write};
use std::panic::{self, PanicInfo};
use std::process::{Stdio, Command};
use std::env;

use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};
use tokio::{select, signal};

use thiserror::Error;

use crossterm::style::{Colors, Color, SetColors};
use crossterm::ExecutableCommand;
use crossterm::terminal::{SetSize, SetTitle};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
struct Server {
    enable: bool,
    address: String,
    apiKey: String,
}

impl Default for Server {
    fn default() -> Self {
        Server {
            enable: true,
            address: "localhost:8000".to_string(),
            apiKey: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ConfigLayout {
    #[serde(with = "humantime_serde")]
    refreshInterval: Duration,
    #[serde(flatten)]
    servers: HashMap<String, Server>,
}

impl Default for ConfigLayout {
    fn default() -> Self {
        let mut map = HashMap::<String, Server>::new();
        map.insert("example-server".into(), Server::default());
        ConfigLayout {
            refreshInterval: Duration::new(30, 0),
            servers: map,
        }
    }
}

struct Handler;
#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        tokio::spawn(server_activity(ctx));
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            let player_list = vec![]; // Replace with actual player list
            let response = format!("Player List: {:?}", player_list);
            let _ = command.create_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseFlags::CHANNEL_MESSAGE_WITH_SOURCE)
                    .interaction_response_data(|message| {
                        message.content(response)
                    })
            }).await;
        }
    }
}

async fn server_activity(ctx: Context) {
    loop {
        let guard = ctx.data.read().await;
        let addr = guard.get::<TMAddress>().unwrap();
        let refresh_interval = guard.get::<TMRefreshInterval>().unwrap();
        let a2s = A2SClient::new().await.unwrap();
        let status: String;
        match a2s.info(addr).await {
            Ok(info) => {
                status = format!("Playing {}/{}", info.players, info.max_players);
            }
            Err(_) => {
                status = "Offline".into();
            }
        }
        ctx.set_activity(Some(ActivityData::custom(status)));
        sleep(refresh_interval.clone()).await;
    }
}

struct TMAddress;
impl TypeMapKey for TMAddress {
    type Value = String;
}

struct TMRefreshInterval;
impl TypeMapKey for TMRefreshInterval {
    type Value = Duration;
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid discord api key '{0}'")]
    InvalidToken(String),
}

async fn watch_server(name: String, server: Server, refresh_interval: Duration) -> anyhow::Result<()> {
    validate(&server.apiKey).map_err(|_| Error::InvalidToken(server.apiKey.clone()))?;
    let mut client = Client::builder(&server.apiKey, GatewayIntents::default()).event_handler(Handler).await?;
    client.data.write().await.insert::<TMAddress>(server.address);
    client.data.write().await.insert::<TMRefreshInterval>(refresh_interval);
    loop {
        match client.start().await {
            Ok(_) => {}
            Err(err) => {
                println!("Server {} crashed: {}. (Attempting restart)", name, err);
            }
        }
    }
}

fn quit() {
    println!("Press Enter to exit...");
    io::stdout().flush().expect("Failed to flush stdout");
    let mut input = String::new();
    io::stdin().read_line(&mut input).expect("Failed to read line");
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    #[cfg(target_os = "windows")]
    {
        let args: Vec<String> = env::args().collect();
        if !args.iter().any(|i| i == "-b") {
            Command::new("conhost")
                .args(["cmd.exe", "/K", "mode con cols=50 lines=10 && player-count-discord-bot.exe -b && exit"])
                .spawn()
                .expect("failed to boostrap onto conhost.");
            return;
        }
    }

    panic::set_hook(Box::new(|msg: &PanicInfo<'_>| {
        println!("{}", msg);
        quit();
    }));
    stdout().execute(SetTitle("Player Count Bots")).unwrap();
    stdout().execute(SetColors(Colors::new(Color::DarkGreen, Color::Black))).unwrap();
    stdout().execute(SetSize(50, 100)).unwrap();

    let config_path = "./config.toml".to_string();
    if fs::metadata(&config_path).is_err() {
        fs::File::create(&config_path).unwrap();
    }
    let toml = fs::read_to_string(&config_path).unwrap();
    let config_doc: toml::Table = toml::from_str(&toml).expect("config file doesn't contain valid TOML");

    let mut config = ConfigLayout::default();
    if config_doc.is_empty() {
        fs::write(&config_path, toml::to_string(&config).unwrap().as_str()).unwrap();
    } else {
        config.servers.drain();
        for (name, value) in config_doc {
            if name == "refreshInterval" {
                if let toml::Value::String(v) = &value {
                    config.refreshInterval = v.parse::<humantime::Duration>().unwrap().into();
                }
            } else if let toml::Value::Table(v) = &value {
                let s = value.try_into::<Server>().unwrap();
                config.servers.insert(name, s);
            }
        }
    }

    let mut set = JoinSet::new();
    for (name, server) in config.servers.iter() {
        set.spawn(watch_server(name.clone(), server.clone(), config.refreshInterval));
    }
    select! {
        _ = signal::ctrl_c() => println!("Ctrl+C received, stopping..."),
        _ = set.join_next() => {},
    }
}

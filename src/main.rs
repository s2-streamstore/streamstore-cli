use account::AccountService;
use basin::BasinService;
use clap::{builder::styling, Parser, Subcommand};
use colored::*;
use config::{config_path, create_config};
use error::S2CliError;
use streamstore::{
    client::{BasinClient, Client, ClientConfig, HostCloud},
    types::BasinMetadata,
};
use tracing_subscriber::{fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt};
use types::{BasinConfig, StreamConfig, RETENTION_POLICY_PATH, STORAGE_CLASS_PATH};

mod account;
mod basin;
mod stream;

mod config;
mod error;
mod types;

const STYLES: styling::Styles = styling::Styles::styled()
    .header(styling::AnsiColor::Green.on_default().bold())
    .usage(styling::AnsiColor::Green.on_default().bold())
    .literal(styling::AnsiColor::Blue.on_default().bold())
    .placeholder(styling::AnsiColor::Cyan.on_default());

const GENERAL_USAGE: &str = color_print::cstr!(
    r#"          
    <dim>$</dim> <bold>s2-cli config set --auth-token ...</bold>
    <dim>$</dim> <bold>s2-cli account list-basins --prefix "bar" --start-after "foo" --limit 100</bold>        
    "#
);

#[derive(Parser, Debug)]
#[command(version, about, override_usage = GENERAL_USAGE, styles = STYLES)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Manage s2-cli configuration
    Config {
        #[command(subcommand)]
        action: ConfigActions,
    },

    /// Manage s2 account
    Account {
        #[command(subcommand)]
        action: AccountActions,
    },

    /// Manage s2 basins
    Basin {
        /// Name of the basin to manage.        
        basin: String,

        #[command(subcommand)]
        action: BasinActions,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigActions {
    /// Set the authentication token
    Set {
        #[arg(short, long)]
        auth_token: String,
    },
}

#[deny(missing_docs)]
#[derive(Subcommand, Debug)]
enum AccountActions {
    /// List basins
    ListBasins {
        /// List basin names that begin with this prefix.
        #[arg(short, long)]
        prefix: Option<String>,

        /// List basins names that lexicographically start after this name.        
        #[arg(short, long)]
        start_after: Option<String>,

        /// Number of results, upto a maximum of 1000.
        #[arg(short, long)]
        limit: Option<usize>,
    },

    /// Create a basin
    CreateBasin {
        /// Basin name, which must be globally unique.        
        basin: String,

        #[command(flatten)]
        config: BasinConfig,
    },

    /// Delete a basin
    DeleteBasin {
        /// Basin name to delete.        
        basin: String,
    },

    /// Get basin config
    GetBasinConfig {
        /// Basin name to get config for.
        basin: String,
    },

    /// Reconfigure a basin
    ReconfigureBasin {
        /// Basin name to reconfigure.        
        basin: String,

        /// Configuration to apply.        
        #[command(flatten)]
        config: BasinConfig,
    },
}

#[derive(Subcommand, Debug)]
enum BasinActions {
    /// List Streams
    ListStreams {
        /// List stream names that begin with this prefix.
        #[arg(short, long)]
        prefix: Option<String>,

        /// List stream names that lexicographically start after this name.        
        #[arg(short, long)]
        start_after: Option<String>,

        /// Number of results, upto a maximum of 1000.
        #[arg(short, long)]
        limit: Option<usize>,
    },

    /// Create a stream
    CreateStream {
        /// Name of the stream to create.
        stream: String,

        /// Configuration to apply.        
        #[command(flatten)]
        config: Option<StreamConfig>,
    },

    /// Delete a stream
    DeleteStream {
        /// Name of the stream to delete.
        stream: String,
    },

    /// Get stream config
    GetStreamConfig {
        /// Name of the stream to get config for.
        stream: String,
    },

    /// Reconfigure a stream
    ReconfigureStream {
        /// Name of the stream to reconfigure.
        stream: String,

        /// Configuration to apply.        
        #[command(flatten)]
        config: StreamConfig,
    },
}

fn s2_config(auth_token: String) -> ClientConfig {
    ClientConfig::new(auth_token.to_string())
        .with_host_uri(HostCloud::Local)
        .with_connection_timeout(std::time::Duration::from_secs(5))
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    miette::set_panic_hook();
    run().await?;
    Ok(())
}

async fn run() -> Result<(), S2CliError> {
    let commands = Cli::parse();
    let config_path = config_path()?;

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .pretty()
                .with_span_events(FmtSpan::NEW)
                .compact()
                .with_writer(std::io::stderr),
        )
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    match commands.command {
        Commands::Config { action } => match action {
            ConfigActions::Set { auth_token } => {
                create_config(&config_path, auth_token)?;
                eprintln!("{}", "✓ Token set successfully".green().bold());
                eprintln!(
                    "  Configuration saved to: {}",
                    config_path.display().to_string().cyan()
                );
            }
        },

        Commands::Account { action } => {
            let cfg = config::load_config(&config_path)?;
            let account_service =
                AccountService::new(Client::connect(s2_config(cfg.auth_token)).await?);
            match action {
                AccountActions::ListBasins {
                    prefix,
                    start_after,
                    limit,
                } => {
                    let response = account_service
                        .list_basins(
                            prefix.unwrap_or_default(),
                            start_after.unwrap_or_default(),
                            limit.unwrap_or_default(),
                        )
                        .await?;

                    for basin_metadata in response.basins {
                        let BasinMetadata { name, state, .. } = basin_metadata;

                        let state = match state {
                            streamstore::types::BasinState::Active => state.to_string().green(),
                            streamstore::types::BasinState::Deleting => state.to_string().red(),
                            _ => state.to_string().yellow(),
                        };
                        println!("{} {}", name, state);
                    }
                }

                AccountActions::CreateBasin { basin, config } => {
                    let (storage_class, retention_policy) = match &config.default_stream_config {
                        Some(config) => {
                            let storage_class = config.storage_class.clone();
                            let retention_policy = config.retention_policy.clone();
                            (storage_class, retention_policy)
                        }
                        None => (None, None),
                    };
                    account_service
                        .create_basin(basin, storage_class, retention_policy)
                        .await?;

                    eprintln!("{}", "✓ Basin created successfully".green().bold());
                }

                AccountActions::DeleteBasin { basin } => {
                    account_service.delete_basin(basin).await?;
                    eprintln!("{}", "✓ Basin deleted successfully".green().bold());
                }

                AccountActions::GetBasinConfig { basin } => {
                    let basin_config = account_service.get_basin_config(basin).await?;
                    let basin_config: BasinConfig = basin_config.into();
                    println!("{}", serde_json::to_string_pretty(&basin_config)?);
                }

                AccountActions::ReconfigureBasin { basin, config } => {
                    let mut mask = Vec::new();
                    match &config.default_stream_config {
                        Some(config) => {
                            if config.storage_class.is_some() {
                                mask.push(STORAGE_CLASS_PATH.to_string());
                            }

                            if config.retention_policy.is_some() {
                                mask.push(RETENTION_POLICY_PATH.to_string());
                            }
                        }
                        None => {}
                    }

                    account_service
                        .reconfigure_basin(basin, config.into(), mask)
                        .await?;
                }
            }
        }

        Commands::Basin { basin, action } => {
            let cfg = config::load_config(&config_path)?;
            let basin_config = s2_config(cfg.auth_token);
            match action {
                BasinActions::ListStreams {
                    prefix,
                    start_after,
                    limit,
                } => {
                    let basin_client = BasinClient::connect(basin_config, basin).await?;
                    let streams = BasinService::new(basin_client)
                        .list_streams(
                            prefix.unwrap_or_default(),
                            start_after.unwrap_or_default(),
                            limit.unwrap_or_default(),
                        )
                        .await?;
                    for stream in streams {
                        println!("{}", stream);
                    }
                }

                BasinActions::CreateStream { stream, config } => {
                    let basin_client = BasinClient::connect(basin_config, basin).await?;
                    BasinService::new(basin_client)
                        .create_stream(stream, config.map(Into::into))
                        .await?;
                    eprintln!("{}", "✓ Stream created successfully".green().bold());
                }

                BasinActions::DeleteStream { stream } => {
                    let basin_client = BasinClient::connect(basin_config, basin).await?;
                    BasinService::new(basin_client)
                        .delete_stream(stream)
                        .await?;
                    eprintln!("{}", "✓ Stream deleted successfully".green().bold());
                }

                BasinActions::GetStreamConfig { stream } => {
                    let basin_client = BasinClient::connect(basin_config, basin).await?;
                    let config = BasinService::new(basin_client)
                        .get_stream_config(stream)
                        .await?;
                    let config: StreamConfig = config.into();
                    println!("{}", serde_json::to_string_pretty(&config)?);
                }

                BasinActions::ReconfigureStream { stream, config } => {
                    let basin_client = BasinClient::connect(basin_config, basin).await?;
                    let mut mask = Vec::new();

                    if config.storage_class.is_some() {
                        mask.push("storage_class".to_string());
                    };

                    if config.retention_policy.is_some() {
                        mask.push("retention_policy".to_string());
                    };

                    BasinService::new(basin_client)
                        .reconfigure_stream(stream, config.into(), mask)
                        .await?;

                    eprintln!("{}", "✓ Stream reconfigured successfully".green().bold());
                }
            }
        }
    }
    Ok(())
}

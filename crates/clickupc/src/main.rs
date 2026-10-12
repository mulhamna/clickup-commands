use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug, Parser)]
#[command(
    name = "clickupc",
    about = "Unofficial ClickUp CLI and MCP server",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging (stderr)
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// MCP server commands
    #[command(subcommand)]
    Mcp(McpCommand),
}

#[derive(Debug, Subcommand)]
enum McpCommand {
    /// Run the MCP server over stdio or Streamable HTTP
    Serve {
        #[arg(long, value_enum, default_value = "stdio")]
        transport: Transport,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8787)]
        port: u16,
        #[arg(long, default_value = "/mcp")]
        path: String,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum Transport {
    Stdio,
    StreamableHttp,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"))
    };
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    match cli.command {
        Commands::Mcp(McpCommand::Serve {
            transport,
            host,
            port,
            path,
        }) => match transport {
            Transport::Stdio => clickupc_mcp::run_stdio().await?,
            Transport::StreamableHttp => {
                let path = if path.starts_with('/') {
                    path
                } else {
                    format!("/{path}")
                };
                clickupc_mcp::run_streamable_http(&host, port, &path).await?;
            }
        },
    }
    Ok(())
}

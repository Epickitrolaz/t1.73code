use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "t1.73code", version = "0.1.73", about = "Ultra low-memory T3 Code CLI & server in Rust supporting the Pi agent")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(short, long, default_value = "3000", help = "Port to listen on")]
    pub port: u16,

    #[arg(long, default_value = "127.0.0.1", help = "Host to listen on")]
    pub host: String,

    #[arg(long, help = "Share server over Tailscale")]
    pub share: bool,

    #[arg(long, help = "Connect server via T3 Connect tunnel")]
    pub connect: bool,

    #[arg(long, help = "Custom T3 home directory")]
    pub home_dir: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Start the t1.73code server")]
    Up {
        #[arg(short, long, default_value = "3000")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long)]
        share: bool,
        #[arg(long)]
        connect: bool,
        #[arg(long)]
        home_dir: Option<String>,
    },
    #[command(about = "Start the t1.73code server")]
    Serve {
        #[arg(short, long, default_value = "3000")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long)]
        share: bool,
        #[arg(long)]
        connect: bool,
        #[arg(long)]
        home_dir: Option<String>,
    },
    #[command(about = "Show pairing link and token")]
    Pair {
        #[arg(long)]
        home_dir: Option<String>,
    },
    #[command(about = "Check prerequisites and system health")]
    Doctor,
    #[command(about = "Manage T3 Connect")]
    Connect {
        #[command(subcommand)]
        action: ConnectAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConnectAction {
    #[command(about = "Link environment to T3 Connect")]
    Link {
        #[arg(long)]
        headless: bool,
        #[arg(long)]
        publish_only: bool,
    },
    #[command(about = "Show T3 Connect status")]
    Status,
    #[command(about = "Unlink environment from T3 Connect")]
    Unlink,
    #[command(about = "Sign in to T3 Connect")]
    Login {
        #[arg(long)]
        headless: bool,
    },
    #[command(about = "Sign out of T3 Connect")]
    Logout,
}

pub async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    println!("t1.73code v0.1.73");
    Ok(())
}

use clap::Parser;
use t1_73code::cli::Cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    t1_73code::cli::run(cli).await
}

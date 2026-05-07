use clap::Parser;
use colored::Colorize;
use shki::cli::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = shki::run(cli).await {
        println!("\n{}: {}", "Error".red(), e);
        std::process::exit(1);
    }
}

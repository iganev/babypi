use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct CliArgs {
    /// Config file
    #[arg(short = 'c', long)]
    pub config: Option<String>,

    /// Debug logging
    #[arg(short = 'v', long, default_value = "false")]
    pub verbose: bool,
}

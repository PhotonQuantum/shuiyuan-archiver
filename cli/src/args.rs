use std::path::PathBuf;

use clap::{ArgGroup, Args, Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about)]
#[command(propagate_version = true)]
pub struct Opts {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Authenticate with Shuiyuan BBS and get the API token.
    Auth {
        /// Do not open the browser automatically.
        #[clap(short, long)]
        no_open: bool,
    },
    /// Archive a topic.
    Archive(Archive),
}

#[derive(Args)]
#[command(group(ArgGroup::new("topic").args(["topic_id", "url"]).required(true)))]
pub struct Archive {
    /// The ID of the topic to archive.
    #[clap(short = 'i', long)]
    pub topic_id: Option<usize>,
    /// The URL of the topic to archive.
    #[clap(short, long)]
    pub url: Option<String>,
    /// The path to save the archive.
    #[clap(short, long)]
    pub save_at: PathBuf,
    /// Whether to mask the username.
    #[clap(short, long)]
    pub anonymous: bool,
    /// API token. You can get one by `auth` command.
    #[clap(short, long)]
    pub token: Option<String>,
}

use std::process::ExitCode;
use std::str::FromStr;

use anyhow::{anyhow, Result};
use clap::Parser;
use console::style;
use once_cell::sync::Lazy;
use regex::Regex;
use tracing_subscriber::EnvFilter;

use sa_core::re_exports::uuid::Uuid;

use crate::args::{Archive, Commands, Opts};
use crate::auth::auth;

mod archive;
mod args;
mod auth;

static APP_ID: Lazy<Uuid> =
    Lazy::new(|| Uuid::from_str("db559e8d-1bb1-4cf1-a5b8-b5cb4e05ea82").unwrap());

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(e) = entry().await {
        println!("{}", style(e).red());
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

async fn entry() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let opts = Opts::parse();
    match opts.command {
        Commands::Auth { no_open } => auth(no_open),
        Commands::Archive(Archive {
            topic_id,
            url,
            save_at,
            anonymous,
            token,
        }) => {
            static RE_URL: Lazy<Regex> =
                Lazy::new(|| Regex::new(r#"https://shuiyuan.sjtu.edu.cn/t/topic/(\d+)"#).unwrap());
            let topic = if let Some(url) = url {
                RE_URL
                    .captures(&url)
                    .and_then(|caps| caps.get(1).expect("regex match").as_str().parse().ok())
                    .ok_or_else(|| anyhow!("Invalid token."))?
            } else {
                topic_id.expect("clap arg match")
            };
            let token = token
                .or_else(|| std::env::var("SHUIYUAN_TOKEN").ok())
                .ok_or_else(|| anyhow!("Missing token. Please specify an API token via `token` argument or `SHUIYUAN_TOKEN` environment variable."))?;
            archive::archive(&token, topic, &save_at, anonymous).await
        }
    }
}

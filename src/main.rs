//! ellidri, your *kawaii* IRC server.

#![forbid(unsafe_code)]
#![warn(clippy::all, rust_2018_idioms)]
#![allow(
    clippy::filter_map,
    clippy::find_map,
    clippy::shadow_unrelated,
    clippy::use_self
)]
#![recursion_limit = "1024"]

use clap::{Arg, Command};
use util::hash_password;

use crate::channel::Channel;
use crate::client::Client;
use crate::config::Config;
use crate::state::State;
use anyhow::{anyhow, Context, Result};
use std::env;
mod channel;
mod client;
mod config;
mod control;
mod data;
#[macro_use]
mod lines;
mod net;
mod state;
mod tls;
mod util;

#[tokio::main(flavor = "multi_thread")]
pub async fn main() -> Result<()> {
    if cfg!(debug_assertions) {
        env::set_var("RUST_BACKTRACE", "1");
    }

    let log_settings = env_logger::Env::new()
        .filter_or("ELLIDRI_LOG", "ellidri=debug")
        .write_style("ELLIDRI_LOG_STYLE");
    env_logger::Builder::from_env(log_settings)
        .format(|buf, r| {
            use std::io::Write;
            writeln!(buf, "[{:<5} {}] {}", r.level(), r.target(), r.args())
        })
        .init();

    let app = Command::new("Ellidri")
        .about("irc server")
        .subcommands(vec![
            Command::new("gen-config")
                .about("generate a new configuration file")
                .arg(
                    Arg::new("output-file")
                        .long("output-file")
                        .help("file to write config file to"),
                ),
            Command::new("start")
                .about("start the ellidri irc server")
                .arg(
                    Arg::new("config")
                        .long("config")
                        .help("path to ellidri config file"),
                ),
            Command::new("hash-password")
                .about("read user input, running it through argon2 hashing"),
        ])
        .get_matches();

    match app.subcommand() {
        Some(("gen-config", gen)) => {
            Config::default()
                .write_to_file(
                    gen.get_one::<String>("output-file")
                        .context("failed to get output-file")?,
                )
                .await?;
        }
        Some(("start", start)) => {
            control::load_config_and_run(
                start
                    .get_one::<String>("config")
                    .context("failed to get config")?
                    .to_string(),
            )
            .await?;
        }
        Some(("hash-password", _)) => {
            let pass = rpassword::prompt_password("input password: ")
                .context("failed to read user input")?;
            let hashed_password = hash_password(&pass).unwrap();
            println!("hashed password: {hashed_password}");
            assert!(crate::util::verify_password_hash(&hashed_password, &pass).is_ok());
        }
        _ => return Err(anyhow!("invalid subcommand")),
    }
    Ok(())
}

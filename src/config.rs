//! Configuration file specification and parsing.
//!
//! # Usage
//!
//! ```rust,ignore
//! let Config { domain, motd, .. } = config::from_file("ellidri.toml");
//! ```

use serde::Deserialize;
use std::path::Path;
use std::{fmt, fs, net, path, process};

use crate::modes::is_channel_mode_string;

/// Options specific to TLS connections.
#[derive(Deserialize)]
pub struct TlsOptions {
    /// The identity file to use as certificate and private key for the TCP socket.
    pub tls_identity: path::PathBuf,
}

/// An address record, with options for TLS connections.
#[derive(Deserialize)]
pub struct BindToAddress {
    /// The IP and TCP port to which to bind.
    pub addr: net::SocketAddr,

    /// The TLS options associated with the IP/TCP port.
    #[serde(flatten)]
    pub tls: Option<TlsOptions>,
}

/// Default bound-to addresses.
fn bind_to_address() -> Vec<BindToAddress> {
    vec![BindToAddress {
        addr: net::SocketAddr::from(([0, 0, 0, 0], 6667)),
        tls: None,
    }]
}

/// Default default chan modes.
fn default_chan_modes() -> String {
    String::from("+nt")
}

/// Default number of threads spawned by the server.
fn worker_threads() -> usize {
    1
}

/// The main configuration. It contains all options read from the configuration
/// file.
#[derive(Deserialize)]
pub struct Config {
    /// The domain of the irc server. Sent to clients in most IRC messages.
    pub domain: String,

    /// The IP and TCP ports to which to bind.
    ///
    /// It is set to *:6667 (clear-text) by default.
    #[serde(default = "bind_to_address")]
    pub bind_to_address: Vec<BindToAddress>,

    /// These modes are set when a channel is created.
    #[serde(default = "default_chan_modes")]
    pub default_chan_mode: String,

    /// The optional log level.
    ///
    /// Valid values are:
    /// - "trace": report incoming and outgoing messages.
    /// - "debug": report clients actions (default for dev builds).
    /// - "info": report new/closed connections (default for releases).
    /// - "warn": report warnings.
    /// - "error": report critical errors/bugs in the program.
    pub log_level: Option<String>,

    /// The message of the day.
    ///
    /// It can span on multiple lines by using three double-quotes (""") at the
    /// beginning and at the end. Empty lines are not ignored, but it is trimmed
    /// (it seems).
    pub motd: Option<String>,

    /// The number of threads spawned by tokio.
    ///
    /// Must be between 1 and 32,768. It is set to 1 by default.
    #[serde(default = "worker_threads")]
    pub worker_threads: usize,
}

fn invalid_config<T, E>(err: E) -> T
    where E: fmt::Display
{
    eprintln!("Oh no... senpai made a mistake in here...");
    eprintln!("Senpai, I don't know what to do with {}", err);
    eprintln!("Please fix that quickly senpai!!");
    eprintln!("        o_O                 THNK Y FR YR PTNC");
    process::exit(1)
}

/// Reads the configuration file at `path`, or exit if there is an error.
///
/// Error cases:
/// - can't open and read the file (e.g. does not exist, missing permissions).
/// - can't decode its contents (e.g. missing value, invalid format).
/// - contents are semantically invalid (e.g. no address given, unexpected log_level string)
pub fn from_file<P>(path: P) -> Config
    where P: AsRef<Path>
{
    let contents = fs::read_to_string(path).unwrap_or_else(|err| {
        eprintln!("Senpai! I can't open your config file!!");
        eprintln!("It looks like {}", err);
        process::exit(1);
    });
    let config: Config = toml::from_str(&contents).unwrap_or_else(|err| {
        invalid_config(err)
    });
    if !(1 <= config.worker_threads && config.worker_threads <= 32768) {
        invalid_config("worker_threads must be between 1 and 32768.")
    }
    if let Some(ref log_level) = config.log_level {
        if log_level != "trace" && log_level != "debug" && log_level != "info"
            && log_level != "warn" && log_level != "error"
        {
            invalid_config(r#"log_level must be "trace", "debug", "info", "warn" or "error"."#)
        }
    }
    if !is_channel_mode_string(&config.default_chan_mode) {
        invalid_config("Bad default_chan_mode")
    }
    if config.bind_to_address.is_empty() {
        invalid_config("No address to bind to")
    }
    config
}

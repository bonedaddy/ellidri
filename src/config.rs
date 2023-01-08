//! Configuration structures.
//!
//! See [`doc/ellidri.conf`][1] on the repository for an explanation of each setting.
//!
//! [1]: https://git.sr.ht/~taiite/ellidri/tree/master/doc/ellidri.conf

use anyhow::{Context, Result};
use ellidri_tokens::mode;
use gethostname::gethostname;
use std::{fmt, io, net, path};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Content(String),
    InvalidDomain,
    InvalidModes,
}

impl Error {
    fn s(message: impl Into<String>) -> Error {
        Error::Content(message.into())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(val: io::Error) -> Self {
        Self::Io(val)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => err.fmt(f),
            Self::Content(message) => message.fmt(f),
            Self::InvalidDomain => write!(f, "'domain' must be a domain name (e.g. irc.com)"),
            Self::InvalidModes => write!(f, "'default_chan_mode' must be a mode string (e.g. +nt)"),
        }
    }
}

/// TLS-related and needed information for TLS bindings.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tls {
    pub certificate: path::PathBuf,
    pub key: path::PathBuf,
}

/// Listening address + port + optional TLS settings.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Binding {
    pub address: net::SocketAddr,
    pub tls: Option<Tls>,
}
/// OPER credentials
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Oper {
    pub name: String,
    pub password: String,
}

/// Settings for `State`.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct State {
    pub domain: String,
    pub org_name: String,
    pub org_location: String,
    pub org_mail: String,
    pub default_chan_mode: String,
    pub motd_file: String,
    pub opers: Vec<Oper>,
    pub password: String,
    pub awaylen: usize,
    pub channellen: usize,
    pub keylen: usize,
    pub kicklen: usize,
    pub namelen: usize,
    pub nicklen: usize,
    pub topiclen: usize,
    pub userlen: usize,
    pub login_timeout: u64,
}

impl Default for State {
    fn default() -> State {
        State {
            domain: String::from(gethostname().to_string_lossy()),
            org_name: String::from("unspecified"),
            org_location: String::from("unspecified"),
            org_mail: String::from("unspecified"),
            default_chan_mode: String::from("+nst"),
            motd_file: String::from("/etc/motd"),
            opers: Vec::new(),
            password: String::new(),
            awaylen: 300,
            channellen: 50,
            keylen: 24,
            kicklen: 300,
            namelen: 64,
            nicklen: 32,
            topiclen: 300,
            userlen: 64,
            login_timeout: 60_000,
        }
    }
}

/// The whole configuration.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Config {
    pub bindings: Vec<Binding>,
    pub workers: usize,
    pub state: State,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            bindings: vec![Binding {
                address: net::SocketAddr::from(([127, 0, 0, 1], 6667)),
                tls: None,
            }],
            workers: 0,
            state: State::default(),
        }
    }
}

impl Config {
    pub async fn from_file(path: &str) -> Result<Self> {
        let config: Self = serde_yaml::from_str(
            &tokio::fs::read_to_string(path)
                .await
                .context("failed to read file")?,
        )
        .context("failed to deserialize config file")?;

        if !mode::is_channel_mode_string(&config.state.default_chan_mode) {
            return Err(Error::InvalidModes.into());
        }
        Ok(config)
    }
    pub async fn write_to_file(&self, path: &str) -> Result<()> {
        let conf = serde_yaml::to_string(self).context("failed to serialize config")?;
        tokio::fs::write(path, conf)
            .await
            .context("failed to write config file")?;
        Ok(())
    }
}

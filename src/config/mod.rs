//! Configuration parsing and structures.
//!
//! See [`doc/ellidri.conf`][1] on the repository for an explanation of each setting.
//!
//! [1]: https://git.sr.ht/~taiite/ellidri/tree/master/doc/ellidri.conf

use self::parser::{Parser, ModeString, Oper};
use std::{fmt, io, net, path};
use std::ops::Range;

mod parser;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Format(Parser, Option<usize>, Range<usize>, String),
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
    fn from(val: io::Error) -> Self { Self::Io(val) }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => err.fmt(f),
            Self::Format(parser, lineno, col, msg) => {
                writeln!(f, "{}", msg)?;
                if let Some(lineno) = lineno {
                    writeln!(f, "     |")?;
                    parser.lines().enumerate()
                        .skip_while(|(lno, _)| lno + 3 < *lineno)
                        .take_while(|(lno, _)| lno <= lineno)
                        .try_for_each(|(lno, line)| writeln!(f, "{:4} | {}", lno + 1, line))?;
                    let start = col.start + 1;
                    let len = col.end - col.start;
                    writeln!(f, "     |{0:1$}{2:^<3$}", ' ', start, '^', len)?;
                }
                Ok(())
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Settings for `State`.
#[derive(Default)]
pub struct State {
    pub domain: String,

    pub default_chan_mode: String,
    pub motd_file: String,
    pub password: Option<String>,
    pub opers: Vec<(String, String)>,

    pub org_name: String,
    pub org_location: String,
    pub org_mail: String,

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

/// Listening address + port + optional TLS settings.
pub struct Binding {
    pub address: net::SocketAddr,
    pub tls_identity: Option<path::PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaslBackend {
    None,
    Database,
}

impl Default for SaslBackend {
    fn default() -> Self { Self::None }
}

impl fmt::Display for SaslBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Database => write!(f, "db"),
        }
    }
}

pub mod db {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Driver {
        #[cfg(feature = "sqlite")]
        Sqlite,
        #[cfg(feature = "postgres")]
        Postgres,
    }

    #[derive(Clone, Debug)]
    pub struct Url(pub Driver, pub String);
}

/// The whole configuration.
#[derive(Default)]
pub struct Config {
    pub bindings: Vec<Binding>,
    #[cfg(feature = "websocket")]
    pub ws_endpoint: Option<net::SocketAddr>,
    pub workers: usize,
    pub state: State,
    pub sasl_backend: SaslBackend,
    pub db_url: Option<db::Url>,
}

impl State {
    pub /*const*/ fn sample() -> Self {
        Self {
            domain: "ellidri.localdomain".to_owned(),
            default_chan_mode: "+nt".to_owned(),
            motd_file: "/etc/motd".to_owned(),
            password: None,
            opers: vec![],
            org_name: "--unspecified--".to_owned(),
            org_location: "--unspecified--".to_owned(),
            org_mail: "--unspecified--".to_owned(),
            awaylen: 300,
            channellen: 50,
            keylen: 24,
            kicklen: 300,
            namelen: 64,
            nicklen: 32,
            topiclen: 300,
            userlen: 64,
            login_timeout: 60000,
        }
    }
}

impl Config {
    pub /*const*/ fn sample() -> Self {
        Self {
            bindings: vec![
                Binding {
                    address: net::SocketAddr::from(([127, 0, 0, 1], 6667)),
                    tls_identity: None,
                }
            ],
            #[cfg(feature = "websocket")]
            ws_endpoint: None,
            workers: 0,
            state: State::sample(),
            sasl_backend: SaslBackend::None,
            db_url: None,
        }
    }

    /// Reads the configuration file at the given path.
    pub fn from_file<P>(path: P) -> Result<Self>
        where P: AsRef<path::Path>
    {
        let mut res = Self::sample();
        let mut default_chan_mode = ModeString(res.state.default_chan_mode.clone());
        let mut opers = Vec::new();
        let mut parser = Parser::read(path)?;

        parser = parser
            .setting("bind_to", |values| res.bindings = values)?
            .setting("oper",    |values| opers = values)?
            .unique_setting("workers",           false, |value| res.workers = value)?
            .unique_setting("domain",            true,  |value| res.state.domain = value)?
            .unique_setting("org_name",          false, |value| res.state.org_name = value)?
            .unique_setting("org_location",      false, |value| res.state.org_location = value)?
            .unique_setting("org_mail",          false, |value| res.state.org_mail = value)?
            .unique_setting("default_chan_mode", false, |value| default_chan_mode = value)?
            .unique_setting("motd_file",         false, |value| res.state.motd_file = value)?
            .unique_setting("password",          false, |value| res.state.password = Some(value))?
            .unique_setting("awaylen",           false, |value| res.state.awaylen = value)?
            .unique_setting("channellen",        false, |value| res.state.channellen = value)?
            .unique_setting("keylen",            false, |value| res.state.keylen = value)?
            .unique_setting("kicklen",           false, |value| res.state.kicklen = value)?
            .unique_setting("namelen",           false, |value| res.state.namelen = value)?
            .unique_setting("nicklen",           false, |value| res.state.nicklen = value)?
            .unique_setting("topiclen",          false, |value| res.state.topiclen = value)?
            .unique_setting("userlen",           false, |value| res.state.userlen = value)?
            .unique_setting("login_timeout",     false, |value| res.state.login_timeout = value)?
            .unique_setting("sasl_backend",      false, |value| res.sasl_backend = value)?;

        let db_needed = res.sasl_backend == SaslBackend::Database;
        parser = parser
            .unique_setting("db_url", db_needed, |value| res.db_url = Some(value))?;

        #[cfg(feature = "websocket")]
        {
            parser = parser
                .unique_setting("ws_endpoint", false, |value| res.ws_endpoint = Some(value))?;
        }

        parser.check_unknown_settings()?;

        res.state.default_chan_mode = default_chan_mode.0;
        for Oper(name, pass) in opers {
            res.state.opers.push((name, pass));
        }

        Ok(res)
    }
}

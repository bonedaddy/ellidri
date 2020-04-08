//! ellidri, your *kawaii* IRC server.
//!
//! # Usage
//!
//! You need a configuration file, and pass its name as an argument. The git repository contains an
//! example `doc/ellidri.conf`, with comments describing the different options.
//!
//! During development: `cargo run -- doc/ellidri.conf`
//!
//! For an optimized build:
//!
//! ```console
//! cargo install
//! ellidri ellidri.conf
//! ```

#![forbid(unsafe_code)]
#![warn(clippy::all, rust_2018_idioms)]
#![allow(clippy::filter_map, clippy::find_map, clippy::shadow_unrelated, clippy::use_self)]

pub use crate::config::Config;
pub use crate::state::State;
pub use ellidri_tokens as tokens;
use std::{env, process};
use std::sync::Arc;
use tokio::sync::Notify;

pub mod auth;
mod channel;
mod client;
pub mod config;
mod lines;
mod net;
mod state;
mod util;

/// The beginning of everything
pub fn start() {
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

    let matches = clap::App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(clap::Arg::with_name("CONFIG_FILE")
            .long("--config")
            .value_name("CONFIG_FILE")
            .help("ellidri's configuration file")
            .required_unless("DOMAIN")
            .conflicts_with("DOMAIN"))
        .arg(clap::Arg::with_name("DOMAIN")
            .long("--domain")
            .value_name("DOMAIN")
            .help("ellidri's effective domain name (unimplemented)")
            .required_unless("CONFIG_FILE")
            .conflicts_with("CONFIG_FILE"))
        .get_matches();

    if matches.is_present("DOMAIN") {
        eprintln!("At the moment, --domain is unimplemented.  Please use --config instead.");
        process::exit(1);
    }

    let config_path = matches.value_of("CONFIG_FILE").unwrap();
    let cfg = Config::from_file(&config_path).unwrap_or_else(|err| {
        log::error!("Failed to read {:?}: {}", config_path, err);
        process::exit(1);
    });

    let sasl_backend = cfg.sasl_backend;
    let auth_provider = auth::choose_provider(sasl_backend, cfg.db_url).unwrap_or_else(|err| {
        log::warn!("Failed to initialize the {} SASL backend: {}", sasl_backend, err);
        Box::new(auth::DummyProvider)
    });
    let mut runtime = runtime(cfg.workers);
    let shared = State::new(cfg.state, auth_provider);

    let num_bindings = cfg.bindings.len();
    let failures = Arc::new(Notify::new());

    let mut store = net::TlsIdentityStore::default();
    for config::Binding { address, tls_identity } in cfg.bindings {
        if let Some(identity_path) = tls_identity {
            let acceptor = store.acceptor(identity_path);
            let server = net::listen_tls(address, shared.clone(), acceptor,
                                         failures.clone());
            runtime.spawn(server);
        } else {
            let server = net::listen(address, shared.clone(),
                                     failures.clone());
            runtime.spawn(server);
        }
    }

    runtime.block_on(control(num_bindings, failures));
}

fn runtime(workers: usize) -> tokio::runtime::Runtime {
    let mut builder = tokio::runtime::Builder::new();

    if workers != 0 {
        builder.core_threads(workers);
    }

    // TODO panic catch

    builder
        .threaded_scheduler()
        .enable_io()
        .enable_time()
        .build()
        .unwrap_or_else(|err| {
            log::error!("Failed to start the tokio runtime: {}", err);
            process::exit(1);
        })
}

async fn control(mut num_bindings: usize, failures: Arc<Notify>) {
    loop {
        if num_bindings == 0 {
            log::error!("No listener left, exiting.");
            return;
        }
        failures.notified().await;
        num_bindings -= 1;
    }
}

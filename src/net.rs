use crate::{lines, State};
use ellidri_reader::IrcReader;
use ellidri_tokens::Message;
use futures::future;
use std::{fs, path, process, str};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::{io, net, sync, time};
use tokio::sync::Notify;
use tokio_tls::TlsAcceptor;

const KEEPALIVE_SECS: u64 = 75;
const TLS_TIMEOUT_SECS: u64 = 30;

/// `TlsAcceptor` cache, to avoid reading the same identity file several times.
#[derive(Default)]
pub struct TlsIdentityStore {
    acceptors: HashMap<path::PathBuf, Arc<tokio_tls::TlsAcceptor>>,
}

impl TlsIdentityStore {
    /// Retrieves the acceptor at `path`, or get it from the cache if it has already been built.
    pub fn acceptor(&mut self, file: path::PathBuf) -> Arc<tokio_tls::TlsAcceptor> {
        if let Some(acceptor) = self.acceptors.get(&file) {
            acceptor.clone()
        } else {
            let acceptor = Arc::new(build_acceptor(&file));
            self.acceptors.insert(file, acceptor.clone());
            acceptor
        }
    }
}

/// Read the file at `p`, parse the identity and builds a `TlsAcceptor` object.
fn build_acceptor(p: &path::Path) -> tokio_tls::TlsAcceptor {
    log::info!("Loading TLS identity from {:?}", p.display());
    let der = fs::read(p).unwrap_or_else(|err| {
        log::error!("Failed to read {:?}: {}", p.display(), err);
        process::exit(1);
    });
    let identity = native_tls::Identity::from_pkcs12(&der, "").unwrap_or_else(|err| {
        log::error!("Failed to parse {:?}: {}", p.display(), err);
        process::exit(1);
    });
    let acceptor = native_tls::TlsAcceptor::builder(identity)
        .min_protocol_version(Some(native_tls::Protocol::Tlsv11))
        .build()
        .unwrap_or_else(|err| {
            log::error!("Failed to initialize TLS: {}", err);
            process::exit(1);
        });
    tokio_tls::TlsAcceptor::from(acceptor)
}

// TODO make listen and listen_tls poll a Notify future and return when they are notified
// https://docs.rs/tokio/0.2.13/tokio/sync/struct.Notify.html

/// Returns a future that listens, accepts and handles incoming plain-text connections.
pub async fn listen(addr: SocketAddr, shared: State, failures: Arc<Notify>) {
    let mut ln = match net::TcpListener::bind(&addr).await {
        Ok(ln) => ln,
        Err(err) => {
            log::error!("Failed to listen to {}: {}", addr, err);
            failures.notify();
            return;
        }
    };

    log::info!("Listening on {} for plain-text connections...", addr);

    loop {
        match ln.accept().await {
            Ok((conn, peer_addr)) => handle_tcp(conn, peer_addr, shared.clone()),
            Err(err) => log::warn!("Failed to accept connection: {}", err),
        }
    }
}

fn handle_tcp(conn: net::TcpStream, peer_addr: SocketAddr, shared: State) {
    if let Err(err) = conn.set_keepalive(Some(time::Duration::from_secs(KEEPALIVE_SECS))) {
        log::warn!("Failed to set TCP keepalive: {}", err);
        return;
    }
    tokio::spawn(handle(conn, peer_addr, shared));
}

/// Returns a future that listens, accepts and handles incoming TLS connections.
pub async fn listen_tls(addr: SocketAddr, shared: State, acceptor: Arc<TlsAcceptor>,
                        failures: Arc<Notify>)
{
    let mut ln = match net::TcpListener::bind(&addr).await {
        Ok(ln) => ln,
        Err(err) => {
            log::error!("Failed to listen to {}: {}", addr, err);
            failures.notify();
            return;
        }
    };

    log::info!("Listening on {} for tls connections...", addr);

    loop { match ln.accept().await {
        Ok((conn, peer_addr)) => handle_tls(conn, peer_addr, shared.clone(), acceptor.clone()),
        Err(err) => log::warn!("Failed to accept connection: {}", err),
    }}
}

fn handle_tls(conn: net::TcpStream, peer_addr: SocketAddr, shared: State,
              acceptor: Arc<TlsAcceptor>)
{
    if let Err(err) = conn.set_keepalive(Some(time::Duration::from_secs(KEEPALIVE_SECS))) {
        log::warn!("Failed to set TCP keepalive for {}: {}", peer_addr, err);
        return;
    }
    tokio::spawn(async move {
        let tls_handshake_timeout = time::Duration::from_secs(TLS_TIMEOUT_SECS);
        let tls_handshake = time::timeout(tls_handshake_timeout, acceptor.accept(conn));
        match tls_handshake.await {
            Ok(Ok(tls_conn)) => handle(tls_conn, peer_addr, shared).await,
            Ok(Err(err)) => log::warn!("TLS handshake with {} failed: {}", peer_addr, err),
            Err(_) => log::warn!("TLS handshake with {} timed out", peer_addr),
        }
    });
}

macro_rules! rate_limit {
    ( $rate:expr, $burst:expr, $do:expr ) => {{
        let rate: u32 = $rate;
        let burst: u32 = $burst;
        let mut used_points: u32 = 0;
        let mut last_round = time::Instant::now();

        loop {
            used_points = match $do.await {
                Ok(points) => used_points + points,
                Err(err) => return Err(err),
            };
            if burst < used_points {
                let elapsed = last_round.elapsed();
                let millis = elapsed.as_millis();
                let millis = if (std::u32::MAX as u128) < millis {
                    std::u32::MAX
                } else {
                    millis as u32
                };

                used_points = used_points.saturating_sub(millis / rate * 4);
                last_round += elapsed;

                if burst < used_points {
                    let wait_millis = (used_points - burst) / 4 * rate;
                    let wait = time::Duration::from_millis(wait_millis as u64);
                    time::delay_for(wait).await;
                    used_points = burst;
                    last_round += wait;
                }
            }
        }
    }};
}

/// Returns a future that handles an IRC connection.
async fn handle<S>(conn: S, peer_addr: SocketAddr, shared: State)
    where S: io::AsyncRead + io::AsyncWrite
{
    let (reader, mut writer) = io::split(conn);
    let mut reader = IrcReader::new(reader, 512);
    let (msg_queue, mut outgoing_msgs) = sync::mpsc::unbounded_channel();
    let peer_id = shared.peer_joined(peer_addr, msg_queue).await;
    tokio::spawn(login_timeout(peer_id, shared.clone()));

    let incoming = async {
        let mut buf = String::new();
        rate_limit!(1024, 16, async {
            buf.clear();
            reader.read_message(&mut buf).await?;
            log::trace!("{} >> {}", peer_addr, buf.trim());
            handle_buffer(peer_id, &buf, &shared).await
        });
        #[allow(unreachable_code)]  // used for type inference
        Ok(())
    };

    let outgoing = async {
        use io::AsyncWriteExt as _;

        while let Some(msg) = outgoing_msgs.recv().await {
            writer.write_all(msg.as_ref()).await?;
        }
        Ok(())
    };

    let res = future::try_join(incoming, outgoing).await;
    shared.peer_quit(peer_id, res.err()).await;
}

/// Handle a line from the client.
///
/// Returns `Err(_)` if the connection must be closed, `Ok(points)` otherwise.  Points are used for
/// rate limits.
async fn handle_buffer(peer_id: usize, buf: &str, shared: &State) -> io::Result<u32> {
    if buf.is_empty() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, lines::CONNECTION_RESET));
    }

    if let Some(msg) = Message::parse(buf) {
        return shared.handle_message(peer_id, msg).await
            .map_err(|()| io::ErrorKind::Other.into());
    }

    Ok(1)
}

async fn login_timeout(peer_id: usize, shared: State) {
    let timeout = shared.login_timeout().await;
    time::delay_for(time::Duration::from_millis(timeout)).await;
    shared.remove_if_unregistered(peer_id).await;
}

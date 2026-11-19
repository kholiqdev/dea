//! Connections kept for reuse. HTTP/1.1 connections come back only after
//! their response body was read; HTTP/2 connections stay in while in use
//! and are shared.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use url::Url;

use crate::connect::Connection;
use crate::proxy::ProxyKey;
use crate::request::Request;
use crate::tls::TlsFingerprint;

/// How long a connection may sit unused before it is closed.
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// Which requests may share a connection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PoolKey {
    scheme: String,
    host: String,
    port: u16,
    /// `None` for plain HTTP, where TLS settings don't matter.
    tls: Option<TlsFingerprint>,
    /// `None` for a direct connection.
    proxy: Option<ProxyKey>,
}

impl PoolKey {
    pub(crate) fn new(request: &Request, proxy: Option<&Url>) -> Self {
        let url = &request.url;
        let tls = (url.scheme() == "https")
            .then(|| TlsFingerprint::new(&request.tls, request.options.verify_tls));
        PoolKey {
            scheme: url.scheme().to_owned(),
            host: url.host_str().unwrap_or_default().to_owned(),
            port: url.port_or_known_default().unwrap_or_default(),
            tls,
            proxy: proxy.map(ProxyKey::new),
        }
    }
}

#[derive(Default)]
pub(crate) struct Pool {
    idle: Mutex<HashMap<PoolKey, Vec<Idle>>>,
}

struct Idle {
    connection: Connection,
    /// When the connection was last handed out or put back.
    since: Instant,
}

impl Pool {
    /// The most recently used open connection for `key`. An HTTP/1.1
    /// connection leaves the pool; an HTTP/2 one is shared and stays.
    pub(crate) fn checkout(&self, key: &PoolKey) -> Option<Connection> {
        let mut idle = self.lock();
        prune(&mut idle);
        let entries = idle.get_mut(key)?;
        let newest = entries.last_mut()?;
        // An HTTP/2 connection's `since` is not refreshed here: handing it
        // out proves nothing about the peer. `touch` does that once a
        // response arrived.
        if let Some(shared) = newest.connection.share() {
            return Some(shared);
        }
        entries.pop().map(|entry| entry.connection)
    }

    /// Keeps `connection` for later requests to `key`, unless it closed.
    pub(crate) fn checkin(&self, key: PoolKey, connection: Connection) {
        if connection.is_closed() {
            return;
        }
        let mut idle = self.lock();
        prune(&mut idle);
        idle.entry(key).or_default().push(Idle {
            connection,
            since: Instant::now(),
        });
    }

    /// Records that `connection` just carried a response, so it counts as
    /// recently used.
    pub(crate) fn touch(&self, key: &PoolKey, connection: &Connection) {
        let mut idle = self.lock();
        if let Some(entry) = idle.get_mut(key).and_then(|entries| {
            entries
                .iter_mut()
                .find(|e| e.connection.same_connection(connection))
        }) {
            entry.since = Instant::now();
        }
    }

    /// Forgets `connection`, so the next request opens a new one.
    pub(crate) fn evict(&self, key: &PoolKey, connection: &Connection) {
        let mut idle = self.lock();
        if let Some(entries) = idle.get_mut(key) {
            entries.retain(|e| !e.connection.same_connection(connection));
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<PoolKey, Vec<Idle>>> {
        self.idle.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Drops connections the server closed or that sat unused too long.
fn prune(idle: &mut HashMap<PoolKey, Vec<Idle>>) {
    idle.retain(|_, entries| {
        entries
            .retain(|entry| !entry.connection.is_closed() && entry.since.elapsed() < IDLE_TIMEOUT);
        !entries.is_empty()
    });
}

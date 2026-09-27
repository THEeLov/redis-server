//! Unix socket server that multiplexes client connections with `epoll`.

use crate::poller::Poller;
use nix::sys::epoll::{EpollEvent, EpollFlags};
use std::{
    fs, io,
    os::{fd::AsRawFd, unix::net::UnixListener},
};
use tracing::{info, trace, warn};
/// Connected clients and their per-connection state.
pub mod client;
use client::{Client, Clients};

const EPOLL_BUFFER: usize = 1024;

/// Server that accepts clients on a [`UnixListener`] and dispatches their
/// events through an `epoll`-based [`Poller`].
pub struct Server {
    listener: UnixListener,
    clients: Clients,
    poller: Poller<EPOLL_BUFFER>,
}

impl Server {
    /// Creates a server listening on the Unix socket at `socket_path` and
    /// registers the listener with the poller under token `0`.
    ///
    /// # Errors
    ///
    /// Returns an error if the socket cannot be set up (see
    /// [`Server::socket_setup`]), the `epoll` instance cannot be created, or
    /// the listener cannot be registered with it.
    pub fn build(socket_path: &str) -> io::Result<Self> {
        let mut server = Self {
            listener: Self::socket_setup(socket_path)?,
            clients: Clients::new(),
            poller: Poller::build()?,
        };

        server
            .poller
            .add_poller(&server.listener, EpollEvent::new(EpollFlags::EPOLLIN, 0))?;
        Ok(server)
    }

    /// Runs the event loop: accepts new clients on the listener and hands
    /// events from connected clients to their handlers. Only returns on error.
    ///
    /// # Errors
    ///
    /// Returns an error if waiting for events fails. Failures to accept or
    /// register a single client are logged and do not stop the loop.
    pub fn handle_connections(&mut self) -> io::Result<()> {
        loop {
            trace!("waiting for events");
            let n = self.poller.wait_poller()?;
            trace!(n, "got events");

            for i in 0..n {
                let token = self.poller.events[i].data();

                // Accept client if listener has POLLIN
                if token == 0 {
                    self.accept_client();
                    continue;
                }

                // Otherwise handle client request
                let Some(client) = self.clients.get_mut_client(&token) else {
                    continue;
                };

                client.handle_client();
            }
        }
    }

    /// Accepts a pending connection on the listener, makes it non-blocking,
    /// registers it with the poller and stores it as a [`Client`].
    ///
    /// Any failure is logged and the connection is dropped.
    #[allow(clippy::cast_sign_loss)]
    fn accept_client(&mut self) {
        let stream = match self.listener.accept() {
            Ok((stream, _)) => stream,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return, // nothing was waiting
            Err(e) => {
                warn!(error = %e, "failed to accept client");
                return;
            }
        };

        let fd = stream.as_raw_fd() as u64;

        if let Err(e) = stream.set_nonblocking(true) {
            warn!(fd, error = %e, "failed to set client non-blocking, dropping it");
            return;
        }

        if let Err(e) = self
            .poller
            .add_poller(&stream, EpollEvent::new(EpollFlags::EPOLLIN, fd))
        {
            warn!(fd, error = %e, "failed to register client with epoll, dropping it");
            return;
        }

        self.clients.add_client(Client::new(stream));
        info!(fd, "accepted client");
    }

    /// Creates the server's listening Unix socket at `path`, first removing
    /// any file left there by a previous run.
    ///
    /// # Errors
    ///
    /// Returns an error if an existing file at `path` cannot be removed, or
    /// if binding fails, for example because the parent directory does not
    /// exist or the process lacks permission to create the socket there.
    pub fn socket_setup(path: &str) -> Result<UnixListener, io::Error> {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }

        UnixListener::bind(path)
    }
}

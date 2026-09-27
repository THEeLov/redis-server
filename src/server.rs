//! Unix socket server that multiplexes client connections with `epoll`.

use crate::poller::Poller;
use nix::sys::epoll::{EpollEvent, EpollFlags};
use std::{
    io,
    os::{fd::AsRawFd, unix::net::UnixListener},
};
use tracing::{info, trace};
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
    /// Creates a server around `listener` and registers the listener with
    /// the poller under token `0`.
    ///
    /// # Errors
    ///
    /// Returns an error if the `epoll` instance cannot be created or the
    /// listener cannot be registered with it.
    pub fn build(listener: UnixListener) -> io::Result<Self> {
        let mut server = Self {
            listener,
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
    /// Returns an error if waiting for events fails or accepting a new
    /// client fails.
    #[allow(clippy::cast_sign_loss)]
    pub fn handle_connections(&mut self) -> io::Result<()> {
        loop {
            trace!("waiting for events");
            let n = self.poller.wait_poller()?;
            trace!(n, "got events");

            for i in 0..n {
                let token = self.poller.events[i].data();

                // Accepting client
                if token == 0 {
                    let new_client = self.accept_client()?;
                    let fd = new_client.stream.as_raw_fd() as u64;

                    let Ok(()) = self
                        .poller
                        .add_poller(&new_client.stream, EpollEvent::new(EpollFlags::EPOLLIN, fd))
                    else {
                        continue;
                    };

                    self.clients.add_client(new_client);

                    continue;
                }

                let Some(client) = self.clients.get_mut_client(&token) else {
                    continue;
                };

                client.handle_client();
            }
        }
    }

    /// Accepts a pending connection on the listener and wraps it in a
    /// non-blocking [`Client`].
    #[allow(clippy::cast_sign_loss)]
    fn accept_client(&self) -> io::Result<Client> {
        let (stream, _) = self.listener.accept()?;
        let fd = stream.as_raw_fd() as u64;
        info!(fd, "accepted client");

        stream.set_nonblocking(true)?;

        Ok(Client::new(stream))
    }
}

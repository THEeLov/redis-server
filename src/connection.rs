use nix::sys::epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollFlags, EpollTimeout};
use std::{
    io::{self, Read},
    os::{fd::AsRawFd, unix::net::UnixListener},
};
use tracing::{debug, error, info, trace};

pub mod accept;
pub mod client;

use client::Clients;

use crate::connection::accept::accept_client;

const EPOLL_BUFFER: usize = 1024;

/// Runs the epoll event loop: accepts new clients on `listener` and reads
/// incoming data from connected clients. Only returns on error.
///
/// # Errors
///
/// Returns an error if the epoll instance cannot be created, the listener
/// cannot be registered with it, waiting for events fails, or accepting a
/// new client fails.
pub fn handle_connections(listener: UnixListener) -> io::Result<()> {
    let mut clients: Clients = Clients::new();

    let epoll = Epoll::new(EpollCreateFlags::empty())?;
    epoll.add(&listener, EpollEvent::new(EpollFlags::EPOLLIN, 0))?;

    let mut events = [EpollEvent::empty(); EPOLL_BUFFER];
    let mut buffer = [0u8; 1024];
    loop {
        trace!("waiting for events");
        let n = epoll.wait(&mut events, EpollTimeout::NONE)?;
        trace!(n, "got events");

        for event in &events[..n] {
            let token = event.data();

            // Accepting client
            if token == 0 {
                let new_client = accept_client(&listener)?;

                if epoll
                    .add(
                        &new_client.stream,
                        EpollEvent::new(EpollFlags::EPOLLIN, new_client.stream.as_raw_fd() as u64),
                    )
                    .is_err()
                {
                    continue;
                }

                clients.add_client(new_client);

                continue;
            }

            // Now lets handle clients
            let Some(client) = clients.get_mut_client(&token) else {
                continue;
            };

            let Ok(nbytes) = client.stream.read(&mut buffer) else {
                error!(fd = client.stream.as_raw_fd(), "error reading from client");
                continue;
            };

            if nbytes == 0 {
                // TODO: handle close by client
            }

            debug!(
                "{nbytes} bytes: {}",
                String::from_utf8_lossy(&buffer[..nbytes])
            );
        }
    }
}

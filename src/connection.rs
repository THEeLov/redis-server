use nix::sys::epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollFlags, EpollTimeout};
use std::{
    io::{self, Read},
    os::{fd::AsRawFd, unix::net::UnixListener},
};
use tracing::*;

pub mod client;

use client::{Client, Clients};

const EPOLL_BUFFER: usize = 1024;

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

            if token == 0 {
                let Ok((stream, sock_addr)) = listener.accept() else {
                    continue;
                };
                let fd = stream.as_raw_fd() as u64;

                info!(fd, "accepted client");

                if stream.set_nonblocking(true).is_err() {
                    continue;
                }

                if epoll
                    .add(&stream, EpollEvent::new(EpollFlags::EPOLLIN, fd))
                    .is_err()
                {
                    continue;
                }

                let new_client = Client::new(stream, sock_addr);

                clients.add_client(new_client);

                debug!("{clients:?}");
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
    Ok(())
}

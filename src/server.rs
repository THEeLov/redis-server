use crate::poller::Poller;
use nix::sys::epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollFlags, EpollTimeout};
use std::{
    io::{self, Read},
    os::{
        fd::{AsFd, AsRawFd},
        unix::net::UnixListener,
    },
};
use tracing::{debug, error, info, trace};
pub mod client;
use client::{Client, Clients};

const EPOLL_BUFFER: usize = 1024;

pub struct Server {
    listener: UnixListener,
    clients: Clients,
    poller: Poller<EPOLL_BUFFER>,
}

impl Server {
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

    pub fn handle_connections(&mut self) -> io::Result<()> {
        let mut buffer = [0u8; 1024];
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

                // Now lets handle clients
                let Some(client) = self.clients.get_mut_client(&token) else {
                    continue;
                };

                client.handle_client();
            }
        }
    }

    #[allow(clippy::cast_sign_loss)]
    fn accept_client(&self) -> io::Result<Client> {
        let (stream, _) = self.listener.accept()?;

        let fd = stream.as_raw_fd() as u64;

        info!(fd, "accepted client");

        stream.set_nonblocking(true)?;

        Ok(Client::new(stream))
    }
}

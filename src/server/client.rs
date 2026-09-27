use std::{
    collections::HashMap,
    io::Read,
    os::{fd::AsRawFd, unix::net::UnixStream},
};
use tracing::{debug, error};

#[derive(Debug)]
pub struct Client {
    pub stream: UnixStream,
    pub input: Vec<u8>,
    pub closed: bool,
}

impl Client {
    #[must_use]
    pub fn new(stream: UnixStream) -> Client {
        Client {
            stream,
            input: Vec::new(),
            closed: false,
        }
    }

    pub fn handle_client(&mut self) {
        let mut buffer = [0u8; 1024];
        let Ok(nbytes) = self.stream.read(&mut buffer) else {
            error!(fd = self.stream.as_raw_fd(), "error reading from client");
            return;
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

#[derive(Default, Debug)]
pub struct Clients {
    clients: HashMap<u64, Client>,
}

impl Clients {
    #[must_use]
    pub fn new() -> Clients {
        Clients {
            clients: HashMap::new(),
        }
    }

    pub fn get_mut_client(&mut self, fd: &u64) -> Option<&mut Client> {
        self.clients.get_mut(fd)
    }

    #[must_use]
    pub fn get_client(&self, fd: &u64) -> Option<&Client> {
        self.clients.get(fd)
    }

    /// Adds a client and registers it under its file descriptor.
    ///
    /// # Panics
    ///
    /// Panics if the client's file descriptor is negative, which the kernel
    /// never produces for an open socket.
    pub fn add_client(&mut self, client: Client) {
        let fd = u64::try_from(client.stream.as_raw_fd()).expect("fds are never negative");

        self.clients.insert(fd, client);
    }

    pub fn remove_client(&mut self, fd: u64) {
        self.clients.remove(&fd);
    }
}

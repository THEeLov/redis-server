use std::{
    collections::HashMap,
    os::{fd::AsRawFd, unix::net::UnixStream},
};

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

use std::{
    collections::HashMap,
    os::{fd::AsRawFd, unix::net::SocketAddr, unix::net::UnixStream},
};

#[derive(Debug)]
pub struct Client {
    pub stream: UnixStream,
    pub input: Vec<u8>,
    pub closed: bool,
    sock_addr: SocketAddr,
}

impl Client {
    pub fn new(stream: UnixStream, sock_addr: SocketAddr) -> Client {
        Client {
            stream,
            sock_addr,
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
    pub fn new() -> Clients {
        Clients {
            clients: HashMap::new(),
        }
    }

    pub fn get_mut_client(&mut self, fd: &u64) -> Option<&mut Client> {
        self.clients.get_mut(fd)
    }

    pub fn get_client(&self, fd: &u64) -> Option<&Client> {
        self.clients.get(fd)
    }

    pub fn add_client(&mut self, client: Client) {
        self.clients
            .insert(client.stream.as_raw_fd() as u64, client);
    }

    pub fn remove_client(&mut self, fd: u64) {
        self.clients.remove(&fd);
    }
}

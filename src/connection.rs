use std::{
    collections::HashMap,
    io::{Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
};
use tracing::{debug, error, warn};

#[derive(Debug)]
pub struct Connection {
    pub stream: UnixStream,
    pub input: Vec<u8>,
    pub closed: bool,
}

impl Connection {
    #[must_use]
    pub fn new(stream: UnixStream) -> Connection {
        Connection {
            stream,
            input: Vec::new(),
            closed: false,
        }
    }

    pub fn handle_connection(&mut self) {
        let mut buffer = [0u8; 1024];
        let Ok(nbytes) = self.stream.read(&mut buffer) else {
            error!(
                fd = self.stream.as_raw_fd(),
                "error reading from Connection"
            );
            self.closed = true;
            return;
        };

        if nbytes == 0 {
            self.closed = true;
        }

        if let Err(e) = self.stream.write_all(b"Thank you\n") {
            warn!(error = %e, "failed to write reply");
            self.closed = true;
        }

        debug!(
            "{nbytes} bytes: {}",
            String::from_utf8_lossy(&buffer[..nbytes])
        );
    }
}

#[derive(Default, Debug)]
pub struct Connections {
    connections: HashMap<u64, Connection>,
}

impl Connections {
    #[must_use]
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    pub fn get_mut_connection(&mut self, fd: u64) -> Option<&mut Connection> {
        self.connections.get_mut(&fd)
    }

    #[must_use]
    pub fn get_connection(&self, fd: u64) -> Option<&Connection> {
        self.connections.get(&fd)
    }

    /// Adds a Connection and registers it under its file descriptor.
    ///
    /// # Panics
    ///
    /// Panics if the Connection's file descriptor is negative, which the kernel
    /// never produces for an open socket.
    pub fn add_connection(&mut self, connection: Connection) {
        let fd = u64::try_from(connection.stream.as_raw_fd()).expect("fds are never negative");

        self.connections.insert(fd, connection);
    }

    pub fn remove_connection(&mut self, fd: u64) -> Option<Connection> {
        self.connections.remove(&fd)
    }
}

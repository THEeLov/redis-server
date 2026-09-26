use nix::sys::epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollFlags, EpollTimeout};
use std::collections::HashMap;
use std::io;
use std::io::Read;
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::{SocketAddr, UnixStream};
use tracing::{debug, error, info, trace, warn};

use redis_server::listener;

const SOCKET_PATH: &str = "/tmp/myredis.sock";
const MAX_CAPACITY: usize = 5;
const EPOLL_BUFFER: usize = 1024;

#[derive(Debug)]
struct Connection {
    stream: UnixStream,
    input: Vec<u8>,
    closed: bool,
    sock_addr: SocketAddr,
}

#[allow(clippy::cast_sign_loss)]
fn main() -> io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let listener = listener::socket_setup(SOCKET_PATH)?;
    info!(path = SOCKET_PATH, "listening");

    let mut clients: HashMap<u64, Connection> = HashMap::new();

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

                clients.insert(
                    fd,
                    Connection {
                        stream,
                        input: Vec::new(),
                        closed: false,
                        sock_addr,
                    },
                );

                debug!("{clients:?}");
                continue;
            }

            // Now lets handle clients
            let Some(client) = clients.get_mut(&token) else {
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

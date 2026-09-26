use crate::connection::client::Client;
use std::os::unix::net::UnixListener;
use std::{io, os::fd::AsRawFd};
use tracing::info;

/// Accepts a pending connection on `listener` and wraps it in a non-blocking [`Client`].
///
/// # Errors
///
/// Returns an error if accepting the connection fails or if the accepted
/// stream cannot be switched to non-blocking mode.
#[allow(clippy::cast_sign_loss)]
pub fn accept_client(listener: &UnixListener) -> io::Result<Client> {
    let (stream, _) = listener.accept()?;

    let fd = stream.as_raw_fd() as u64;

    info!(fd, "accepted client");

    stream.set_nonblocking(true)?;

    Ok(Client::new(stream))
}

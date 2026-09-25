use std::fs;
use std::io;
use std::os::unix::net::UnixListener;

/// Creates and binds the server's listening socket.
///
/// # Errors
///
/// Returns an error if the address is already in use or
/// the process lacks permission to bind to the port.
pub fn socket_setup(path: &str) -> Result<UnixListener, io::Error> {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }

    UnixListener::bind(path)
}

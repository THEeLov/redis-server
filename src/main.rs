use redis_server::connection::handle_connections;
use std::io;
use tracing::info;

use redis_server::listener;

const SOCKET_PATH: &str = "/tmp/myredis.sock";

fn main() -> io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let listener = listener::socket_setup(SOCKET_PATH)?;
    info!(path = SOCKET_PATH, "listening");

    handle_connections(listener)?;

    Ok(())
}

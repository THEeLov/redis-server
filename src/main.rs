use redis_server::server::Server;
use std::io;
use tracing::info;

const SOCKET_PATH: &str = "/tmp/myredis.sock";

fn main() -> io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut server = Server::build(SOCKET_PATH)?;
    info!(path = SOCKET_PATH, "listening");

    server.handle_connections()?;

    Ok(())
}

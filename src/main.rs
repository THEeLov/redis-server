mod command;
mod parser;
mod redis;
mod reply;

use redis::Redis;
use socket_epoll_server::Server;

const SOCKET_PATH: &str = "/tmp/myredis.sock";

fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    Server::bind(SOCKET_PATH, Redis::new())?.run()
}

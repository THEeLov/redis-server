use socket_epoll_server::{Connection, Handler, Server};
use std::collections::HashMap;

const SOCKET_PATH: &str = "/tmp/myredis.sock";

struct Redis {
    db: HashMap<Vec<u8>, Vec<u8>>,
}

impl Handler for Redis {
    type State = ();

    fn on_data(&mut self, conn: &mut Connection<()>, input: &[u8]) -> usize {
        println!(
            "{} bytes: {:?}",
            input.len(),
            String::from_utf8_lossy(input)
        );
        conn.send(b"+OK\n");
        input.len()
    }
}

fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let redis = Redis { db: HashMap::new() };
    Server::bind(SOCKET_PATH, redis)?.run()
}

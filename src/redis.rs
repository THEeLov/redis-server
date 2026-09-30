//! The Redis server logic: turns bytes received from a client into
//! commands, runs them against the database and sends back the replies.

use crate::{
    command::{Command, Db},
    parser::{Parsed, parse_command},
    reply::Reply,
};
use socket_epoll_server::{Connection, Handler};
use tracing::info;

/// A Redis server holding one in-memory database shared by all clients.
#[derive(Default)]
pub struct Redis {
    db: Db,
}

impl Redis {
    /// Creates a server with an empty database.
    pub fn new() -> Self {
        Self::default()
    }

    /// Interprets one parsed request and runs it against the database.
    fn execute(&mut self, args: Vec<Vec<u8>>) -> Reply {
        let printable: Vec<_> = args
            .iter()
            .map(|arg| arg.escape_ascii().to_string())
            .collect();
        info!("command: {printable:?}");

        match Command::from_args(args) {
            Ok(command) => command.execute(&mut self.db),
            Err(e) => e.into(),
        }
    }
}

impl Handler for Redis {
    type State = ();

    /// Runs every complete command in `input` and queues its reply. A
    /// protocol error gets an error reply and closes the connection.
    fn on_data(&mut self, conn: &mut Connection<()>, input: &[u8]) -> usize {
        let mut consumed = 0;
        loop {
            match parse_command(&input[consumed..]) {
                Ok(Parsed::Command { args, consumed: n }) => {
                    consumed += n;
                    let reply = self.execute(args);
                    conn.send(&reply.to_bytes());
                }
                Ok(Parsed::Incomplete { consumed: n }) => return consumed + n,
                Err(e) => {
                    let reply = Reply::Error(format!("ERR Protocol error: {e}"));
                    conn.send(&reply.to_bytes());
                    conn.close();
                    return input.len();
                }
            }
        }
    }
}

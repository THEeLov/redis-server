//! Interprets parsed requests as Redis commands and runs them against the
//! database.
//!
//! A request arrives from the [parser](crate::parser) as a list of raw
//! arguments. [`Command::from_args`] checks the command name and the number
//! of arguments and builds a typed [`Command`]. [`Command::execute`] then
//! applies it to the [`Db`] and produces the [`Reply`] for the client.
//!
//! Supported commands: `PING [message]`, `ECHO message`, `GET key`,
//! `SET key value`, `DEL key [key ...]`, `EXISTS key [key ...]`, `INCR key`
//! and `DECR key`.
//! Command names are case-insensitive, as in Redis.

use crate::reply::Reply;
use std::{collections::HashMap, error::Error, fmt};

/// The key-value store that commands operate on.
pub type Db = HashMap<Vec<u8>, Vec<u8>>;

/// A command with its arguments checked.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// `PING [message]`: replies `PONG`, or `message` if one is given.
    Ping(Option<Vec<u8>>),
    /// `ECHO message`: replies `message`.
    Echo(Vec<u8>),
    /// `GET key`: replies the value stored at `key`, or null.
    Get(Vec<u8>),
    /// `SET key value`: stores `value` at `key`, replacing any old value.
    Set {
        /// The key to store the value under.
        key: Vec<u8>,
        /// The value to store.
        value: Vec<u8>,
    },
    /// `DEL key [key ...]`: removes the keys and replies how many existed.
    Del(Vec<Vec<u8>>),
    /// `EXISTS key [key ...]`: replies how many of the keys exist. A key
    /// given twice is counted twice, as in Redis.
    Exists(Vec<Vec<u8>>),
    /// `INCR key` and `DECR key`: adds `by` to the integer stored at `key`
    /// (a missing key counts as 0) and replies the new value.
    IncrBy {
        /// The key holding the integer.
        key: Vec<u8>,
        /// `1` for `INCR`, `-1` for `DECR`.
        by: i64,
    },
}

/// A request that is valid RESP but not a valid command. The connection
/// stays open; the client just gets an error reply.
#[derive(Debug, PartialEq, Eq)]
pub enum CommandError {
    /// The command name is not supported. Holds the name as sent, with
    /// non-printable bytes escaped.
    Unknown(String),
    /// The command got the wrong number of arguments. Holds the command
    /// name in lowercase.
    WrongArity(&'static str),
    /// The arguments do not form a valid call, for example `SET` options
    /// that are not supported.
    Syntax,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(name) => write!(f, "ERR unknown command '{name}'"),
            Self::WrongArity(name) => {
                write!(f, "ERR wrong number of arguments for '{name}' command")
            }
            Self::Syntax => f.write_str("ERR syntax error"),
        }
    }
}

impl Error for CommandError {}

impl From<CommandError> for Reply {
    fn from(error: CommandError) -> Self {
        Self::Error(error.to_string())
    }
}

impl Command {
    /// Builds a command from a request's arguments: the command name
    /// followed by its arguments.
    ///
    /// # Errors
    ///
    /// Returns a [`CommandError`] if the command is not supported or its
    /// arguments are not valid.
    pub fn from_args(args: Vec<Vec<u8>>) -> Result<Self, CommandError> {
        let mut args = args.into_iter();
        let name = args.next().unwrap_or_default();
        let mut rest: Vec<Vec<u8>> = args.collect();

        let command = match name.to_ascii_lowercase().as_slice() {
            b"ping" => match rest.len() {
                0 => Self::Ping(None),
                1 => Self::Ping(rest.pop()),
                _ => return Err(CommandError::WrongArity("ping")),
            },
            b"echo" => {
                let [message] = exactly(rest, "echo")?;
                Self::Echo(message)
            }
            b"get" => {
                let [key] = exactly(rest, "get")?;
                Self::Get(key)
            }
            b"set" => match <[Vec<u8>; 2]>::try_from(rest) {
                Ok([key, value]) => Self::Set { key, value },
                Err(rest) if rest.len() > 2 => return Err(CommandError::Syntax),
                Err(_) => return Err(CommandError::WrongArity("set")),
            },
            b"del" if !rest.is_empty() => Self::Del(rest),
            b"del" => return Err(CommandError::WrongArity("del")),
            b"exists" if !rest.is_empty() => Self::Exists(rest),
            b"exists" => return Err(CommandError::WrongArity("exists")),
            b"incr" => {
                let [key] = exactly(rest, "incr")?;
                Self::IncrBy { key, by: 1 }
            }
            b"decr" => {
                let [key] = exactly(rest, "decr")?;
                Self::IncrBy { key, by: -1 }
            }
            _ => return Err(CommandError::Unknown(name.escape_ascii().to_string())),
        };
        Ok(command)
    }

    /// Runs the command against `db` and returns the reply for the client.
    pub fn execute(self, db: &mut Db) -> Reply {
        match self {
            Self::Ping(None) => Reply::Simple("PONG"),
            Self::Ping(Some(message)) | Self::Echo(message) => Reply::Bulk(message),
            Self::Get(key) => db
                .get(&key)
                .map_or(Reply::Null, |value| Reply::Bulk(value.clone())),
            Self::Set { key, value } => {
                db.insert(key, value);
                Reply::Simple("OK")
            }
            Self::Del(keys) => {
                Reply::count(keys.iter().filter(|key| db.remove(*key).is_some()).count())
            }
            Self::Exists(keys) => {
                Reply::count(keys.iter().filter(|key| db.contains_key(*key)).count())
            }
            Self::IncrBy { key, by } => incr_by(db, key, by),
        }
    }
}

/// Adds `by` to the integer stored at `key` and replies the new value. The
/// value must be a decimal integer that fits in an `i64`, as in Redis.
fn incr_by(db: &mut Db, key: Vec<u8>, by: i64) -> Reply {
    let current = match db.get(&key) {
        None => 0,
        Some(value) => match std::str::from_utf8(value).ok().and_then(parse_i64) {
            Some(n) => n,
            None => return Reply::Error("ERR value is not an integer or out of range".into()),
        },
    };
    let Some(new) = current.checked_add(by) else {
        return Reply::Error("ERR increment or decrement would overflow".into());
    };
    db.insert(key, new.to_string().into_bytes());
    Reply::Integer(new)
}

/// Parses an integer the way Redis does: no `+` sign, no spaces and no
/// leading zeros, so that the value round-trips exactly.
fn parse_i64(text: &str) -> Option<i64> {
    let n: i64 = text.parse().ok()?;
    (n.to_string() == text).then_some(n)
}

/// Takes exactly `N` arguments for the command `name`.
fn exactly<const N: usize>(
    args: Vec<Vec<u8>>,
    name: &'static str,
) -> Result<[Vec<u8>; N], CommandError> {
    args.try_into().map_err(|_| CommandError::WrongArity(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<Vec<u8>> {
        words.iter().map(|word| word.as_bytes().to_vec()).collect()
    }

    /// Parses and runs one command, as the server does for each request.
    fn run(db: &mut Db, words: &[&str]) -> Reply {
        match Command::from_args(args(words)) {
            Ok(command) => command.execute(db),
            Err(e) => e.into(),
        }
    }

    fn bulk(data: &str) -> Reply {
        Reply::Bulk(data.as_bytes().to_vec())
    }

    #[test]
    fn names_are_case_insensitive() {
        assert_eq!(
            Command::from_args(args(&["gEt", "k"])),
            Ok(Command::Get(b"k".to_vec()))
        );
    }

    #[test]
    fn ping_and_echo() {
        let mut db = Db::new();
        assert_eq!(run(&mut db, &["PING"]), Reply::Simple("PONG"));
        assert_eq!(run(&mut db, &["PING", "hi"]), bulk("hi"));
        assert_eq!(run(&mut db, &["ECHO", "hello world"]), bulk("hello world"));
    }

    #[test]
    fn set_then_get() {
        let mut db = Db::new();
        assert_eq!(run(&mut db, &["GET", "k"]), Reply::Null);
        assert_eq!(run(&mut db, &["SET", "k", "v1"]), Reply::Simple("OK"));
        assert_eq!(run(&mut db, &["SET", "k", "v2"]), Reply::Simple("OK"));
        assert_eq!(run(&mut db, &["GET", "k"]), bulk("v2"));
    }

    #[test]
    fn del_and_exists_count_keys() {
        let mut db = Db::new();
        run(&mut db, &["SET", "a", "1"]);
        run(&mut db, &["SET", "b", "2"]);

        assert_eq!(run(&mut db, &["EXISTS", "a", "a", "x"]), Reply::Integer(2));
        assert_eq!(run(&mut db, &["DEL", "a", "a", "x"]), Reply::Integer(1));
        assert_eq!(run(&mut db, &["EXISTS", "a", "b"]), Reply::Integer(1));
        assert_eq!(run(&mut db, &["GET", "a"]), Reply::Null);
    }

    #[test]
    fn incr_and_decr() {
        let mut db = Db::new();
        assert_eq!(run(&mut db, &["INCR", "n"]), Reply::Integer(1));
        assert_eq!(run(&mut db, &["INCR", "n"]), Reply::Integer(2));
        assert_eq!(run(&mut db, &["GET", "n"]), bulk("2"));
        assert_eq!(run(&mut db, &["DECR", "m"]), Reply::Integer(-1));
        run(&mut db, &["SET", "n", "-10"]);
        assert_eq!(run(&mut db, &["DECR", "n"]), Reply::Integer(-11));
    }

    #[test]
    fn incr_rejects_non_integers_and_overflow() {
        let mut db = Db::new();
        let not_integer = Reply::Error("ERR value is not an integer or out of range".into());
        for value in ["abc", "1.5", " 1", "+1", "01", "", "99999999999999999999"] {
            run(&mut db, &["SET", "k", value]);
            assert_eq!(run(&mut db, &["INCR", "k"]), not_integer, "{value:?}");
            assert_eq!(run(&mut db, &["GET", "k"]), bulk(value), "value unchanged");
        }
        run(&mut db, &["SET", "k", &i64::MAX.to_string()]);
        assert_eq!(
            run(&mut db, &["INCR", "k"]),
            Reply::Error("ERR increment or decrement would overflow".into())
        );
    }

    #[test]
    fn wrong_number_of_arguments() {
        for (words, name) in [
            (&["PING", "a", "b"][..], "ping"),
            (&["ECHO"], "echo"),
            (&["GET"], "get"),
            (&["GET", "a", "b"], "get"),
            (&["SET", "k"], "set"),
            (&["DEL"], "del"),
            (&["EXISTS"], "exists"),
            (&["INCR"], "incr"),
            (&["DECR", "a", "b"], "decr"),
        ] {
            assert_eq!(
                Command::from_args(args(words)),
                Err(CommandError::WrongArity(name)),
                "{words:?}"
            );
        }
    }

    #[test]
    fn set_options_are_a_syntax_error() {
        assert_eq!(
            Command::from_args(args(&["SET", "k", "v", "EX", "10"])),
            Err(CommandError::Syntax)
        );
    }

    #[test]
    fn unknown_command() {
        assert_eq!(
            run(&mut Db::new(), &["FLY", "away"]),
            Reply::Error("ERR unknown command 'FLY'".into())
        );
    }

    #[test]
    fn unknown_command_name_is_escaped() {
        assert_eq!(
            Command::from_args(args(&["a\r\nb"])),
            Err(CommandError::Unknown("a\\r\\nb".into()))
        );
    }
}

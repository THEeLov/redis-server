//! Replies sent back to clients, encoded in RESP2.

/// A reply to one command.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    /// A short status such as `OK`. Must not contain `\r` or `\n`.
    Simple(&'static str),
    /// An error message that starts with an error code, such as
    /// `ERR syntax error`. Line breaks are replaced with spaces when it is
    /// encoded, so the reply always stays on one line.
    Error(String),
    /// A signed integer, for example the number of keys a command changed.
    Integer(i64),
    /// Binary-safe data, for example a stored value.
    Bulk(Vec<u8>),
    /// No value, for example when a key does not exist.
    Null,
}

impl Reply {
    /// An [`Integer`](Reply::Integer) reply holding a count.
    pub fn count(n: usize) -> Self {
        Self::Integer(i64::try_from(n).unwrap_or(i64::MAX))
    }

    /// Encodes the reply as RESP2 bytes, ready to send to the client.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::Simple(status) => format!("+{status}\r\n").into_bytes(),
            Self::Error(message) => {
                format!("-{}\r\n", message.replace(['\r', '\n'], " ")).into_bytes()
            }
            Self::Integer(n) => format!(":{n}\r\n").into_bytes(),
            Self::Bulk(data) => {
                let mut out = format!("${}\r\n", data.len()).into_bytes();
                out.extend_from_slice(data);
                out.extend_from_slice(b"\r\n");
                out
            }
            Self::Null => b"$-1\r\n".to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_every_reply_type() {
        assert_eq!(Reply::Simple("OK").to_bytes(), b"+OK\r\n");
        assert_eq!(
            Reply::Error("ERR syntax error".into()).to_bytes(),
            b"-ERR syntax error\r\n"
        );
        assert_eq!(Reply::Integer(-3).to_bytes(), b":-3\r\n");
        assert_eq!(
            Reply::Bulk(b"a\r\nb".to_vec()).to_bytes(),
            b"$4\r\na\r\nb\r\n"
        );
        assert_eq!(Reply::Bulk(Vec::new()).to_bytes(), b"$0\r\n\r\n");
        assert_eq!(Reply::Null.to_bytes(), b"$-1\r\n");
    }

    #[test]
    fn error_stays_on_one_line() {
        assert_eq!(
            Reply::Error("ERR bad\r\nthing".into()).to_bytes(),
            b"-ERR bad  thing\r\n"
        );
    }
}

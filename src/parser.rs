//! Parser for Redis client requests.
//!
//! Only RESP requests are accepted: an array of bulk strings, which is
//! what real Redis clients send. `GET key` is sent as
//! `*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n`. Every argument carries its own
//! length, so arguments are binary safe and may contain spaces or `\r\n`.
//!
//! [`parse_command`] does not buffer anything itself. It looks at the bytes
//! it is given and either returns one complete command together with how
//! many bytes it used, or reports that more input is needed.
//!
//! # How a request is parsed
//!
//! ```text
//! *2\r\n$3\r\nGET\r\n$3\r\nkey\r\n
//! └─┬─┘ └─┬─┘ └┬┘   └─┬─┘ └┬┘
//!   │     │    │      │    └─ exactly 3 bytes: "key"
//!   │     │    │      └────── bulk header: next arg is 3 bytes
//!   │     │    └───────────── exactly 3 bytes: "GET"
//!   │     └────────────────── bulk header: next arg is 3 bytes
//!   └──────────────────────── array header: 2 arguments follow
//! ```
//!
//! Headers are found by searching for `\r\n`. Argument bytes are never
//! searched: the parser jumps over exactly `len` bytes, which is why
//! arguments may contain `\r\n` themselves.

use std::{error::Error, fmt};

/// Longest `*<count>` or `$<len>` header line accepted without a
/// terminator
const MAX_LINE_LEN: usize = 64 * 1024;

/// Largest number of arguments in one RESP request.
const MAX_ARGS: usize = 1024 * 1024;

/// Largest single bulk string argument (512 MiB).
const MAX_BULK_LEN: usize = 512 * 1024 * 1024;

/// Upper bound on how many arguments are preallocated from a request's
/// declared count, so a client cannot make the server allocate a huge
/// vector just by sending a large `*<count>` header.
const MAX_PREALLOC_ARGS: usize = 64;

/// Arguments of one request and the number of input bytes it took up.
/// The arguments may be empty, for `*0\r\n` or `*-1\r\n`.
type Frame = (Vec<Vec<u8>>, usize);

/// Result of a successful call to [`parse_command`].
#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    /// A complete command: the command name followed by its arguments.
    /// `args` is never empty.
    Command {
        /// The command name and its arguments, in order.
        args: Vec<Vec<u8>>,
        /// Number of input bytes the command took up.
        consumed: usize,
    },
    /// The input does not contain a complete command yet.
    Incomplete {
        /// Number of leading bytes that held only empty arrays (`*0\r\n` or
        /// `*-1\r\n`). They can be dropped. This is usually `0`.
        consumed: usize,
    },
}

/// A request that breaks the protocol. After one of these the stream can no
/// longer be split into commands reliably, so the connection should be
/// closed.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The request does not start with `*`; holds the type byte that was
    /// found instead.
    ExpectedArray(u8),
    /// The `*<count>` header is not a number, or the count is too large.
    InvalidMultibulkLength,
    /// The `$<len>` header is not a number, is negative, or is too large.
    InvalidBulkLength,
    /// An element of a RESP request is not a bulk string; holds the type
    /// byte that was found instead of `$`.
    ExpectedBulk(u8),
    /// A bulk string is not followed by `\r\n`.
    MissingBulkTerminator,
    /// A header line is longer than the limit and still has no terminator.
    LineTooLong,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedArray(found) => {
                write!(f, "expected '*', got '{}'", [*found].escape_ascii())
            }
            Self::InvalidMultibulkLength => f.write_str("invalid multibulk length"),
            Self::InvalidBulkLength => f.write_str("invalid bulk length"),
            Self::ExpectedBulk(found) => {
                write!(f, "expected '$', got '{}'", [*found].escape_ascii())
            }
            Self::MissingBulkTerminator => f.write_str("expected CRLF after bulk string"),
            Self::LineTooLong => f.write_str("too big request line"),
        }
    }
}

impl Error for ParseError {}

/// Parses the first command in `input`.
///
/// Empty arrays at the start of `input` (`*0\r\n` or `*-1\r\n`) are
/// skipped. Bytes after the first command are not looked at, so call this
/// again on the remaining input to get the next command.
///
/// # Errors
///
/// Returns a [`ParseError`] if `input` breaks the protocol.
pub fn parse_command(input: &[u8]) -> Result<Parsed, ParseError> {
    let mut start = 0;
    loop {
        let rest = &input[start..];
        let parsed = match rest.first() {
            None => None,
            Some(b'*') => parse_multibulk(rest)?,
            Some(&found) => return Err(ParseError::ExpectedArray(found)),
        };

        match parsed {
            None => return Ok(Parsed::Incomplete { consumed: start }),
            Some((args, used)) if args.is_empty() => start += used,
            Some((args, used)) => {
                return Ok(Parsed::Command {
                    args,
                    consumed: start + used,
                });
            }
        }
    }
}

/// Parses a RESP array of bulk strings. `input` starts with `*`.
///
/// Returns the arguments and the number of bytes used, or `None` if the
/// request is not complete yet. A count of zero or less gives no arguments.
fn parse_multibulk(input: &[u8]) -> Result<Option<Frame>, ParseError> {
    let Some((header, mut pos)) = read_crlf_line(input)? else {
        return Ok(None);
    };
    let count = parse_int(&header[1..]).ok_or(ParseError::InvalidMultibulkLength)?;
    if count <= 0 {
        return Ok(Some((Vec::new(), pos)));
    }
    let count = usize::try_from(count)
        .ok()
        .filter(|&count| count <= MAX_ARGS)
        .ok_or(ParseError::InvalidMultibulkLength)?;

    let mut args = Vec::with_capacity(count.min(MAX_PREALLOC_ARGS));
    for _ in 0..count {
        let rest = &input[pos..];
        match rest.first() {
            None => return Ok(None),
            Some(b'$') => {}
            Some(&found) => return Err(ParseError::ExpectedBulk(found)),
        }

        let Some((header, header_len)) = read_crlf_line(rest)? else {
            return Ok(None);
        };
        let len = parse_int(&header[1..])
            .and_then(|len| usize::try_from(len).ok())
            .filter(|&len| len <= MAX_BULK_LEN)
            .ok_or(ParseError::InvalidBulkLength)?;

        let data_start = pos + header_len;
        let data_end = data_start + len;
        let Some(terminator) = input.get(data_end..data_end + 2) else {
            return Ok(None);
        };
        if terminator != b"\r\n" {
            return Err(ParseError::MissingBulkTerminator);
        }

        args.push(input[data_start..data_end].to_vec());
        pos = data_end + 2;
    }

    Ok(Some((args, pos)))
}

/// Finds the first line in `input` ending with `\r\n`.
///
/// Returns the line without its terminator and the number of bytes up to
/// and including the terminator, or `None` if there is no complete line yet.
fn read_crlf_line(input: &[u8]) -> Result<Option<(&[u8], usize)>, ParseError> {
    match input.windows(2).position(|pair| pair == b"\r\n") {
        Some(end) => Ok(Some((&input[..end], end + 2))),
        None if input.len() > MAX_LINE_LEN => Err(ParseError::LineTooLong),
        None => Ok(None),
    }
}

/// Parses a decimal integer such as `3` or `-1`, as used in RESP headers.
fn parse_int(digits: &[u8]) -> Option<i64> {
    if digits.first() == Some(&b'+') {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(args: &[&[u8]], consumed: usize) -> Parsed {
        Parsed::Command {
            args: args.iter().map(|arg| arg.to_vec()).collect(),
            consumed,
        }
    }

    #[test]
    fn non_array_request_is_rejected() {
        assert_eq!(
            parse_command(b"PING\r\n"),
            Err(ParseError::ExpectedArray(b'P'))
        );
        assert_eq!(
            parse_command(b"\r\n"),
            Err(ParseError::ExpectedArray(b'\r'))
        );
    }

    #[test]
    fn empty_input_is_incomplete() {
        assert_eq!(parse_command(b""), Ok(Parsed::Incomplete { consumed: 0 }));
    }

    #[test]
    fn resp_command() {
        let input = b"*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n";
        assert_eq!(
            parse_command(input),
            Ok(command(&[b"GET", b"key"], input.len()))
        );
    }

    #[test]
    fn resp_arguments_are_binary_safe() {
        let input = b"*2\r\n$4\r\nECHO\r\n$4\r\na\r\nb\r\n";
        assert_eq!(
            parse_command(input),
            Ok(command(&[b"ECHO", b"a\r\nb"], input.len()))
        );
    }

    #[test]
    fn resp_empty_bulk_string() {
        let input = b"*2\r\n$4\r\nECHO\r\n$0\r\n\r\n";
        assert_eq!(
            parse_command(input),
            Ok(command(&[b"ECHO", b""], input.len()))
        );
    }

    #[test]
    fn every_prefix_of_resp_command_is_incomplete() {
        let input = b"*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n";
        for end in 0..input.len() {
            assert_eq!(
                parse_command(&input[..end]),
                Ok(Parsed::Incomplete { consumed: 0 }),
                "prefix of length {end}"
            );
        }
    }

    #[test]
    fn only_first_command_is_consumed() {
        let first = b"*1\r\n$4\r\nPING\r\n";
        let mut input = first.to_vec();
        input.extend_from_slice(b"*1\r\n$4\r\nECHO\r\n");
        assert_eq!(parse_command(&input), Ok(command(&[b"PING"], first.len())));
    }

    #[test]
    fn empty_resp_arrays_are_skipped() {
        assert_eq!(
            parse_command(b"*0\r\n*-1\r\n*1\r\n$4\r\nPING\r\n"),
            Ok(command(&[b"PING"], 23))
        );
        assert_eq!(
            parse_command(b"*0\r\n*-1\r\n"),
            Ok(Parsed::Incomplete { consumed: 9 })
        );
    }

    #[test]
    fn invalid_multibulk_length() {
        for input in [&b"*x\r\n"[..], b"*\r\n", b"*+1\r\n", b"*99999999999\r\n"] {
            assert_eq!(
                parse_command(input),
                Err(ParseError::InvalidMultibulkLength),
                "{}",
                input.escape_ascii()
            );
        }
    }

    #[test]
    fn invalid_bulk_length() {
        for input in [
            &b"*1\r\n$x\r\n"[..],
            b"*1\r\n$-1\r\n",
            b"*1\r\n$999999999999\r\n",
        ] {
            assert_eq!(
                parse_command(input),
                Err(ParseError::InvalidBulkLength),
                "{}",
                input.escape_ascii()
            );
        }
    }

    #[test]
    fn non_bulk_element_is_rejected() {
        assert_eq!(
            parse_command(b"*1\r\n:5\r\n"),
            Err(ParseError::ExpectedBulk(b':'))
        );
    }

    #[test]
    fn bulk_string_without_crlf_is_rejected() {
        assert_eq!(
            parse_command(b"*1\r\n$4\r\nPINGxx"),
            Err(ParseError::MissingBulkTerminator)
        );
    }

    #[test]
    fn overlong_header_is_rejected() {
        let mut header = b"*1".to_vec();
        header.resize(MAX_LINE_LEN + 1, b'1');
        assert_eq!(parse_command(&header), Err(ParseError::LineTooLong));
    }

    #[test]
    fn error_messages_match_redis() {
        assert_eq!(
            ParseError::ExpectedBulk(b':').to_string(),
            "expected '$', got ':'"
        );
        assert_eq!(
            ParseError::ExpectedArray(b'P').to_string(),
            "expected '*', got 'P'"
        );
        assert_eq!(
            ParseError::InvalidMultibulkLength.to_string(),
            "invalid multibulk length"
        );
    }
}

# redis-server

A small Redis-compatible key-value server written in Rust. It listens on a
Unix domain socket and speaks **RESP2**, the protocol real Redis uses, so
it works with any client that can send RESP over a Unix socket.

Everything is stored in memory in one database shared by all clients. Data
is lost when the server stops.

## Running

```sh
cargo run
# with logs:
RUST_LOG=info cargo run
```

The server listens on **`/tmp/myredis.sock`**. Any file already at that
path is removed on start.

Quick check with the official client, if installed:

```sh
redis-cli -s /tmp/myredis.sock PING
```

## Connecting

- Connect to the Unix stream socket `/tmp/myredis.sock` (in Rust:
  `std::os::unix::net::UnixStream::connect`).
- One connection can send any number of commands. Keep it open and reuse it.
- Every command gets exactly one reply, in the order the commands were sent.
  You may send several commands before reading replies (pipelining).
- The server may deliver a reply in several chunks, and one read may contain
  more than one reply. Read replies by their format (see below), not by
  "one `read` call = one reply".

## Sending a command

A command is sent as a **RESP array of bulk strings**: the command name,
then its arguments. Nothing else is accepted; plain text such as `GET key\n`
is rejected.

```
*<number of elements>\r\n
$<byte length of element 1>\r\n<element 1>\r\n
$<byte length of element 2>\r\n<element 2>\r\n
...
```

`SET greeting "hello world"` is sent as:

```
*3\r\n$3\r\nSET\r\n$8\r\ngreeting\r\n$11\r\nhello world\r\n
```

Notes:

- Lengths are in **bytes**, not characters (`"čau"` is 4 bytes in UTF-8).
- Arguments are binary safe. They may contain spaces, `\r`, `\n` or any
  byte, because the length says where each one ends.
- Command names are case-insensitive: `GET`, `get` and `GeT` are the same.
- The line terminator is always `\r\n`, not just `\n`.

Encoding in Rust:

```rust
fn encode_command(args: &[&[u8]]) -> Vec<u8> {
    let mut out = format!("*{}\r\n", args.len()).into_bytes();
    for arg in args {
        out.extend_from_slice(format!("${}\r\n", arg.len()).as_bytes());
        out.extend_from_slice(arg);
        out.extend_from_slice(b"\r\n");
    }
    out
}
```

If your client reads a line from the user, split it into words and encode
the words:

```rust
let words: Vec<&[u8]> = line.split_whitespace().map(str::as_bytes).collect();
stream.write_all(&encode_command(&words))?;
```

(Splitting on whitespace means values typed this way cannot contain spaces.
The protocol itself allows them.)

## Reading a reply

Every reply starts with one type byte and ends with `\r\n`:

| First byte | Type          | Example on the wire     | Meaning                               |
| ---------- | ------------- | ----------------------- | ------------------------------------- |
| `+`        | Simple string | `+OK\r\n`               | Status text, the rest of the line     |
| `-`        | Error         | `-ERR syntax error\r\n` | Error message, the rest of the line   |
| `:`        | Integer       | `:2\r\n`                | Signed decimal integer                |
| `$`        | Bulk string   | `$5\r\nhello\r\n`       | `5` bytes of data follow, then `\r\n` |
| `$`        | Null          | `$-1\r\n`               | No value (for example a missing key)  |

How to read one reply:

1. Read one line up to and including `\r\n`.
2. Look at the first byte:
   - `+`, `-` or `:`: the rest of the line is the value. Done.
   - `$`: parse the rest of the line as a number `n`.
     - `n == -1`: the reply is null. Done.
     - otherwise read exactly `n` bytes of data, then 2 more bytes (`\r\n`)
       that are not part of the value.

Do not read a bulk string by searching for the next `\r\n`, because the data
itself may contain `\r\n`. Always use the length.

Reading in Rust, with the stream wrapped in a `BufReader`:

```rust
use std::io::{self, BufRead};

#[derive(Debug)]
enum Reply {
    Simple(String),
    Error(String),
    Integer(i64),
    Bulk(Vec<u8>),
    Null,
}

fn bad<E>(_: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "bad reply")
}

fn read_reply(reader: &mut impl BufRead) -> io::Result<Reply> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    let line = line.trim_end_matches("\r\n");
    let (kind, rest) = line.split_at(1);

    Ok(match kind {
        "+" => Reply::Simple(rest.to_string()),
        "-" => Reply::Error(rest.to_string()),
        ":" => Reply::Integer(rest.parse().map_err(bad)?),
        "$" => match rest.parse::<i64>().map_err(bad)? {
            -1 => Reply::Null,
            len => {
                let len = usize::try_from(len).map_err(bad)?;
                let mut data = vec![0; len + 2]; // data + "\r\n"
                reader.read_exact(&mut data)?;
                data.truncate(len);
                Reply::Bulk(data)
            }
        },
        _ => return Err(bad(kind)),
    })
}
```

`read_line` needs valid UTF-8 for the header line, which every header is.
Only bulk data may be arbitrary bytes, and that is read with `read_exact`.

## Commands

| Command                | Reply                                                                 |
| ---------------------- | --------------------------------------------------------------------- |
| `PING`                 | `+PONG`                                                               |
| `PING message`         | bulk string `message`                                                 |
| `ECHO message`         | bulk string `message`                                                 |
| `SET key value`        | `+OK`. Replaces any existing value.                                   |
| `GET key`              | bulk string with the value, or null (`$-1`) if the key does not exist |
| `DEL key [key ...]`    | integer: how many of the keys existed and were removed                |
| `EXISTS key [key ...]` | integer: how many of the keys exist. A key listed twice counts twice. |

`SET` options such as `EX`, `PX`, `NX` or `XX` are not supported; sending
them gives `ERR syntax error`. Keys and values are byte strings; there are
no other data types (lists, hashes, ...).

### Example session

```
→ *1\r\n$4\r\nPING\r\n
← +PONG\r\n

→ *3\r\n$3\r\nSET\r\n$1\r\nk\r\n$2\r\nhi\r\n
← +OK\r\n

→ *2\r\n$3\r\nGET\r\n$1\r\nk\r\n
← $2\r\nhi\r\n

→ *3\r\n$6\r\nEXISTS\r\n$1\r\nk\r\n$4\r\nnope\r\n
← :1\r\n

→ *2\r\n$3\r\nDEL\r\n$1\r\nk\r\n
← :1\r\n

→ *2\r\n$3\r\nGET\r\n$1\r\nk\r\n
← $-1\r\n
```

## Errors

There are two kinds of errors, and they behave differently:

**Command errors.** The request was valid RESP, but the command is wrong.
You get an error reply and **the connection stays open**:

| Reply                                              | Cause                                                  |
| -------------------------------------------------- | ------------------------------------------------------ |
| `-ERR unknown command 'NAME'`                      | The command is not supported                           |
| `-ERR wrong number of arguments for 'get' command` | Too few or too many arguments (lowercase command name) |
| `-ERR syntax error`                                | `SET` with extra arguments                             |

**Protocol errors.** The bytes are not a valid RESP request, so the server
cannot tell where the next command starts. You get one error reply and then
**the server closes the connection**. Reconnect to continue.

| Reply                                                  | Cause                                                             |
| ------------------------------------------------------ | ----------------------------------------------------------------- |
| `-ERR Protocol error: expected '*', got 'G'`           | The request is not a RESP array, for example plain text `GET key` |
| `-ERR Protocol error: invalid multibulk length`        | The `*<count>` is not a number, or too large                      |
| `-ERR Protocol error: expected '$', got ':'`           | An array element is not a bulk string                             |
| `-ERR Protocol error: invalid bulk length`             | The `$<len>` is not a number, negative, or too large              |
| `-ERR Protocol error: expected CRLF after bulk string` | The data is not exactly `len` bytes followed by `\r\n`            |
| `-ERR Protocol error: too big request line`            | A `*` or `$` header line is longer than 64 KiB                    |

If a protocol error happens, the usual cause is a wrong length (for example
counting characters instead of bytes) or `\n` used instead of `\r\n`.

## Limits

| Limit                              | Value     |
| ---------------------------------- | --------- |
| Arguments per command              | 1,048,576 |
| Size of one argument               | 512 MiB   |
| Length of a `*` or `$` header line | 64 KiB    |

An incomplete command is not an error: the server waits until the rest
arrives. A client that sends half a command and stops will just get no
reply.

## Project layout

| File             | Role                                                     |
| ---------------- | -------------------------------------------------------- |
| `src/main.rs`    | Sets up logging and starts the server                    |
| `src/redis.rs`   | Connects the parts to the socket server (`Handler` impl) |
| `src/parser.rs`  | Bytes → command arguments (RESP request parsing)         |
| `src/command.rs` | Arguments → `Command`, and running it on the database    |
| `src/reply.rs`   | Encodes replies as RESP                                  |

Networking (accepting clients, `epoll`, buffering) is handled by the
[`socket-epoll-server`](https://crates.io/crates/socket-epoll-server) crate.

# Changelog

All notable changes to this project are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project follows [Semantic Versioning](https://semver.org/).

## [0.1.1] - 2026-09-30

### Added

- `INCR key` and `DECR key`: add or subtract one from the integer stored at
  `key` and reply with the new value. A missing key counts as `0`.
- New error replies for these commands:
  `ERR value is not an integer or out of range` when the stored value is not
  a 64-bit decimal integer, and `ERR increment or decrement would overflow`
  when the result would leave the 64-bit range.

## [0.1.0] - 2026-09-30

### Added

- In-memory key-value server listening on the Unix socket
  `/tmp/myredis.sock`, speaking RESP2.
- Commands: `PING`, `ECHO`, `GET`, `SET`, `DEL` and `EXISTS`.
- Command errors reply with `-ERR ...` and keep the connection open;
  protocol errors reply once and close the connection.

[0.1.1]: https://github.com/THEeLov/redis-server/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/THEeLov/redis-server/releases/tag/v0.1.0

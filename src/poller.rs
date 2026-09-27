//! Thin wrapper around a Linux `epoll` instance.
//!
//! Callers identify file descriptors by `u64` tokens and never see `nix`
//! types; all of `epoll` is contained in this module.

use nix::sys::epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollFlags, EpollTimeout};
use std::{io, os::fd::AsFd};

/// An `epoll` instance paired with a fixed-size buffer that holds up to `N`
/// ready events per call to [`Poller::wait`].
pub struct Poller<const N: usize> {
    poller: Epoll,
    events: [EpollEvent; N],
}

impl<const N: usize> Poller<N> {
    /// Creates a new `epoll` instance with an empty event buffer.
    ///
    /// # Errors
    ///
    /// Returns an error if the `epoll` instance cannot be created.
    pub fn build() -> io::Result<Self> {
        Ok(Self {
            poller: Epoll::new(EpollCreateFlags::empty())?,
            events: [EpollEvent::empty(); N],
        })
    }

    /// Registers `fd` for readability, so that [`Poller::wait`] reports
    /// `token` whenever `fd` has data to read.
    ///
    /// # Errors
    ///
    /// Returns an error if `fd` is already registered or cannot be added to
    /// the `epoll` instance.
    pub fn register<Fd: AsFd>(&mut self, fd: Fd, token: u64) -> io::Result<()> {
        self.poller
            .add(&fd, EpollEvent::new(EpollFlags::EPOLLIN, token))?;
        Ok(())
    }

    /// Unregisters `fd` from the `epoll` instance.
    ///
    /// # Errors
    ///
    /// Returns an error if `fd` is not registered or cannot be removed from
    /// the `epoll` instance.
    pub fn deregister<Fd: AsFd>(&mut self, fd: Fd) -> io::Result<()> {
        self.poller.delete(&fd)?;
        Ok(())
    }

    /// Blocks until at least one registered file descriptor is ready and
    /// returns the tokens of the ready ones, at most `N` per call.
    ///
    /// # Errors
    ///
    /// Returns an error if waiting on the `epoll` instance fails.
    pub fn wait(&mut self) -> io::Result<Vec<u64>> {
        let n = self.poller.wait(&mut self.events, EpollTimeout::NONE)?;
        Ok(self.events[..n].iter().map(EpollEvent::data).collect())
    }
}

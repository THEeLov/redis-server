//! Thin wrapper around a Linux `epoll` instance.

use nix::sys::epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollTimeout};
use std::{io, os::fd::AsFd};

/// An `epoll` instance paired with a fixed-size buffer of `N` events that
/// [`Poller::wait_poller`] fills in.
pub struct Poller<const N: usize> {
    poller: Epoll,
    /// Buffer of ready events written by the last call to [`Poller::wait_poller`].
    /// Only the first `n` entries are valid, where `n` is the value it returned.
    pub events: [EpollEvent; N],
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

    /// Registers `fd` with the `epoll` instance, watching for the flags in
    /// `event` and tagging it with the event's data.
    ///
    /// # Errors
    ///
    /// Returns an error if `fd` is already registered or cannot be added to
    /// the `epoll` instance.
    pub fn add_poller<Fd: AsFd>(&mut self, fd: Fd, event: EpollEvent) -> io::Result<()> {
        self.poller.add(&fd, event)?;
        Ok(())
    }

    /// Unregisters `fd` from the `epoll` instance.
    ///
    /// # Errors
    ///
    /// Returns an error if `fd` is not registered or cannot be removed from
    /// the `epoll` instance.
    pub fn remove_poller<Fd: AsFd>(&mut self, fd: Fd) -> io::Result<()> {
        self.poller.delete(&fd)?;
        Ok(())
    }

    /// Blocks until at least one registered file descriptor is ready, then
    /// stores the ready events in [`Poller::events`].
    ///
    /// Returns the number of events written to the buffer.
    ///
    /// # Errors
    ///
    /// Returns an error if waiting on the `epoll` instance fails.
    pub fn wait_poller(&mut self) -> io::Result<usize> {
        self.poller
            .wait(&mut self.events, EpollTimeout::NONE)
            .map_err(io::Error::from)
    }
}

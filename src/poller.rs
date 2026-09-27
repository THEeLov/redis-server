use nix::sys::epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollFlags, EpollTimeout};
use std::{io, os::fd::AsFd};

pub struct Poller<const N: usize> {
    poller: Epoll,
    pub events: [EpollEvent; N],
}

impl<const N: usize> Poller<N> {
    pub fn build() -> io::Result<Self> {
        Ok(Self {
            poller: Epoll::new(EpollCreateFlags::empty())?,
            events: [EpollEvent::empty(); N],
        })
    }

    pub fn add_poller<Fd: AsFd>(&mut self, fd: Fd, event: EpollEvent) -> io::Result<()> {
        self.poller.add(&fd, event)?;
        Ok(())
    }

    pub fn remove_poller<Fd: AsFd>(&mut self, fd: Fd) -> io::Result<()> {
        self.poller.delete(&fd)?;
        Ok(())
    }

    pub fn wait_poller(&mut self) -> io::Result<usize> {
        self.poller
            .wait(&mut self.events, EpollTimeout::NONE)
            .map_err(io::Error::from)
    }
}

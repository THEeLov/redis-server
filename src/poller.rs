struct Poller {
    poller: Epoll,
    events: [EpollEvent; 1024],
}

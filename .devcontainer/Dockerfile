FROM rust:slim-bullseye

ENV DEBIAN_FRONTEND=noninteractive
ENV container=docker

# systemd + dbus + tooling (procps for ps, though script won't rely on it)
RUN apt-get update && apt-get install -y --no-install-recommends \
    systemd systemd-sysv dbus dbus-user-session \
    build-essential pkg-config git ca-certificates curl \
    libssl-dev libdbus-1-dev libsystemd-dev \
    procps \
 && rm -rf /var/lib/apt/lists/*

VOLUME ["/sys/fs/cgroup"]
STOPSIGNAL SIGRTMIN+3
CMD ["/sbin/init"]

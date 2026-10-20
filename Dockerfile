# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

FROM docker.io/library/debian:forky

ENV DEBIAN_FRONTEND=noninteractive \
    HOME=/root \
    USER=root

ENV DEV_CONTAINER=1

# uc and uc-sync from a named build context, e.g.
#   podman build --build-context uc=target/x86_64-unknown-linux-musl/release .
COPY --from=uc uc uc-sync /usr/local/bin/

# Clones the checkout, then builds and installs it with what it needs.
RUN uc sync -t usecode -d

WORKDIR /root/src/github.com/evgnomon/usecode
CMD ["bash"]

FROM docker.io/library/debian:forky

ENV DEBIAN_FRONTEND=noninteractive \
    HOME=/root \
    USER=root

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        git \
        make \
        procps \
        sudo \
    && rm -rf /var/lib/apt/lists/*

ENV DEV_CONTAINER=1

COPY play.sh /tmp/usecode-play.sh

RUN bash /tmp/usecode-play.sh \
    && rm -f /tmp/usecode-play.sh

WORKDIR /build
CMD ["usecode"]

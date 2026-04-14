FROM debian:trixie AS python-builder

ENV PYTHON_VERSION=3.14.3

RUN apt update && apt install -y \
    build-essential \
    curl \
    libbz2-dev \
    libffi-dev \
    libgdbm-dev \
    liblzma-dev \
    libncurses5-dev \
    libreadline-dev \
    libsqlite3-dev \
    libssl-dev \
    tk-dev \
    uuid-dev \
    xz-utils \
    zlib1g-dev

RUN curl -fsSL https://www.python.org/ftp/python/${PYTHON_VERSION}/Python-${PYTHON_VERSION}.tgz -o - | tar -xzv -C /tmp && \
    cd /tmp/Python-${PYTHON_VERSION} && \
    ./configure --enable-optimizations --enable-shared --prefix=/opt/python && \
    make -j$(nproc) && \
    make install && \
    rm -rf /tmp/Python-${PYTHON_VERSION}

FROM debian:trixie

# Define environment variables for versions and paths
ENV NODE_VERSION=v24.13.1 \
    GO_VERSION=1.26.0 \
    GH_CLI_VERSION=2.87.2 \
    GOLANGCI_LINT_VERSION=latest \
    DOCKER_COMPOSE_VERSION=5.0.2 \
    ZIG_VERSION=0.16.0 \
    PYTHON_VERSION=3.14.3

# Copy Python 3.14 built from source
COPY --from=python-builder /opt/python /opt/python

# Register the Python shared library so the dynamic linker can find it
RUN echo "/opt/python/lib" > /etc/ld.so.conf.d/python.conf && ldconfig

ENV PATH="/root/.cargo/bin:/opt/go-${GO_VERSION}/bin:/root/go/bin:/opt/node-${NODE_VERSION}-linux-x64/bin:/opt/python/bin:/opt/zig-x86_64-linux-${ZIG_VERSION}:$PATH"

# Install necessary packages
RUN apt update && apt install -y curl git clang cmake gettext libbz2-dev libreadline-dev libedit-dev zlib1g-dev pkg-config xz-utils unzip libpcre2-dev \
  libvirt-dev libisoburn-dev libisofs-dev libburn-dev


# Install Node.js
RUN curl -fsSL https://nodejs.org/dist/${NODE_VERSION}/node-${NODE_VERSION}-linux-x64.tar.xz -o - | tar --lzma -xv -C /opt

# Install Go
RUN curl -fsSL https://go.dev/dl/go${GO_VERSION}.linux-amd64.tar.gz -o - | tar -xzv -C /opt  --transform "s/^go/go-${GO_VERSION}/"

# Install Docker Compose
RUN curl -L "https://github.com/docker/compose/releases/download/v${DOCKER_COMPOSE_VERSION}/docker-compose-$(uname -s)-$(uname -m)" -o /usr/local/bin/docker-compose && chmod +x /usr/local/bin/docker-compose

# Install GitHub CLI
RUN curl -fsSL https://github.com/cli/cli/releases/download/v${GH_CLI_VERSION}/gh_${GH_CLI_VERSION}_linux_amd64.tar.gz -o - | tar -xzv -C /usr/local  --transform "s/^gh_${GH_CLI_VERSION}_linux_amd64\///"

# Install GolangCI-Lint
RUN go install github.com/golangci/golangci-lint/cmd/golangci-lint@${GOLANGCI_LINT_VERSION}

# Install Zig
RUN curl -fsSL https://ziglang.org/builds/zig-x86_64-linux-${ZIG_VERSION}.tar.xz -o - | tar --lzma -xv -C /opt

# Install Docker and jq
RUN apt update && apt install -y docker.io jq

# Install Ansible
RUN /opt/python/bin/pip3 install ansible uv

ENV ANSIBLE_COLLECTIONS_PATH=/opt/ansible/collections \
    CATAMARAN_VERSION=0.2.18 \
    EGET_VERSION=v1.3.4 \
    MKDEB_VERSION=0.3.0

RUN /opt/python/bin/pip3 install catamaran==${CATAMARAN_VERSION} hcloud poetry \
  && ansible-galaxy collection install evgnomon.catamaran --collections-path ${ANSIBLE_COLLECTIONS_PATH} \
  && go install github.com/zyedidia/eget@${EGET_VERSION} \
  && go install github.com/digitalocean/doctl/cmd/doctl@latest

COPY evgnomon.asc /etc/apt/trusted.gpg.d/
COPY evgnomon.sources /etc/apt/sources.list.d/

RUN apt-get update && apt-get install -y mkdeb=$MKDEB_VERSION tree

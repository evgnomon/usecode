FROM ubuntu:jammy
WORKDIR /app

RUN apt update && apt install -y sudo git make build-essential libssl-dev zlib1g-dev libbz2-dev libreadline-dev libsqlite3-dev curl libncurses5-dev libncursesw5-dev xz-utils  libffi-dev liblzma-dev
RUN DEBIAN_FRONTEND=noninteractive TZ=Etc/UTC apt-get -y install tzdata tk-dev

RUN useradd -m nouser && passwd -d nouser && adduser nouser sudo
USER nouser

RUN git clone https://github.com/pyenv/pyenv.git /home/nouser/.pyenv
RUN echo 'export PYENV_ROOT="$HOME/.pyenv"' >> /home/nouser/.bashrc
RUN echo 'command -v pyenv >/dev/null || export PATH="$PYENV_ROOT/bin:$PATH"' >> /home/nouser/.bashrc
RUN echo 'eval "$(pyenv init -)"' >> /home/nouser/.bashrc


ENV PYENV_ROOT "/home/nouser/.pyenv"
ENV PATH "/home/nouser/.pyenv/shims:$PYENV_ROOT/bin:$PATH"

RUN PYTHON_CONFIGURE_OPTS="--enable-shared" pyenv install 3.12.0 2.7.18
RUN pyenv global 3.12.0 2.7.18
RUN pip install --upgrade pip
RUN pip install ansible
RUN sudo apt install -y zip unzip

ADD . .
ADD docker /home/nouser
RUN sudo chown nouser:nouser /home/nouser/.ssh
RUN mkdir -p /home/nouser/.local/bin
RUN ansible-playbook -i /home/nouser/inventory main.yaml

CMD ["/usr/bin/bash"]

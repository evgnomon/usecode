# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

FROM mysql:8.4.4
RUN curl --ipv4 -sSLf https://dev.mysql.com/get/Downloads/MySQL-Router/mysql-router-8.4.4-linux-glibc2.28-x86_64.tar.xz \
  -o /opt/mysql-router.tar.xz \
  && tar -xJvf /opt/mysql-router.tar.xz -C /opt/ \
  && rm /opt/mysql-router.tar.xz \
  && cp -r /opt/mysql-router-8.4.4-linux-glibc2.28-x86_64/bin /usr/ \
  && cp -r /opt/mysql-router-8.4.4-linux-glibc2.28-x86_64/lib /usr/

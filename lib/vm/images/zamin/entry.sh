#!/usr/bin/env bash


export DEV_CONTAINER=1

useradd -m -s /bin/bash zamin
echo "zamin ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/zamin
chmod 0440 /etc/sudoers.d/zamin

#!/usr/bin/env bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

python client.py --url https://localhost:8084 --email hg@evgnomon.org --json --pass-file <(getsecret | jq -r '.vaultwarden.local.pass')

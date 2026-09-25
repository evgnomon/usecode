#!/bin/sh
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

CONTEXT_NAME=$(repofqn)
TOKEN=$(getsecret $CONTEXT_NAME | jq -r ".hetzner.prod")

echo 'active_context = "'$CONTEXT_NAME'"
[[contexts]]
  name = "'$CONTEXT_NAME'"
  token = "'$TOKEN'"
'

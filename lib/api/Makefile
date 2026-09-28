# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := usecode-agent-api
EXTRA_SOURCES := $(shell find migrations templates static model_image -type f)

include ../rust.mk

.PHONY: run

run:
	@cargo run -p $(NAME)

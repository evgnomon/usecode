# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

from usecode.kit.foo import foo


def test_foo():
    assert foo("foo") == "foo"

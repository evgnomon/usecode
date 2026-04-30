#!/bin/bash
set -euo pipefail

BUILD_DIR="${BUILD_DIR:?BUILD_DIR must be set}"
PREFIX="${PREFIX:-/usr/local}"
STAGE="$BUILD_DIR$PREFIX"

export CDPATH=
export CFLAGS="-O3 -pipe -fno-plt -flto -U_FORTIFY_SOURCE -D_FORTIFY_SOURCE=1"
export LDFLAGS="-rdynamic -Wl,-O1 -Wl,--as-needed -flto"
export LD_LIBRARY_PATH=/usr/local/lib:$LD_LIBRARY_PATH

unset PYENV_VERSION
unset PYENV_ROOT

PYTHON3_CONFIG_DIR="$(/usr/local/bin/python3 -c 'import sysconfig; print(sysconfig.get_config_var("LIBPL"))')"

if [ ! -f "$STAGE/bin/vim" ] || [ configure -nt "$STAGE/bin/vim" ]; then
    ./configure \
        --with-features=huge \
        --disable-nls \
        --enable-multibyte \
        --enable-python3interp=yes \
        --with-python3-command="/usr/local/bin/python3" \
        --with-python3-config-dir="$PYTHON3_CONFIG_DIR" \
        --enable-cscope \
        --prefix="$PREFIX"
fi

make
make install DESTDIR="$BUILD_DIR"

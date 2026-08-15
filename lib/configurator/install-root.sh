#!/bin/sh

set -e

IS_WSL2=$(grep -qi "microsoft" /proc/version 2>/dev/null && echo true || echo false)

apt update && apt install -y git
apt upgrade -y

apt install -y make build-essential libssl-dev zlib1g-dev libbz2-dev libreadline-dev libsqlite3-dev curl libncurses5-dev libncursesw5-dev xz-utils tk-dev libffi-dev liblzma-dev hwdata pinentry-tty usbutils unzip libyaml-dev pkg-config libgdbm-dev libgdbm-compat-dev

if [ "$IS_WSL2" = "true" ] && [ -x /usr/bin/sudo.ws ]; then
  update-alternatives --set sudo /usr/bin/sudo.ws
fi

if [ "$IS_WSL2" != "true" ]; then
  apt install -y yubikey-manager scdaemon ykcs11 libpcsclite-dev swig pcscd libpam-u2f libpam-yubico
  USBIP_BIN=$(ls /usr/lib/linux-tools/*/usbip 2>/dev/null | tail -n1)
  [ -n "$USBIP_BIN" ] && update-alternatives --install /usr/local/bin/usbip usbip "$USBIP_BIN" 20
  curl -sSL https://raw.githubusercontent.com/Yubico/libfido2/main/udev/70-u2f.rules | tee /etc/udev/rules.d/70-u2f.rules > /dev/null

  mkdir -p /etc/polkit-1/rules.d
  cat > /etc/polkit-1/rules.d/90-pcscd.rule << 'EOF'
polkit.addRule(function(action, subject) {
    if ((action.id == "org.debian.pcsc-lite.access_pcsc" || action.id == "org.debian.pcsc-lite.access_card" ) && subject.isInGroup("plugdev")) {
        return polkit.Result.YES;
    }

});
EOF
fi

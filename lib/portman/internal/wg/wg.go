// Package wg manages the WireGuard interface using the ip(8) and wg(8)
// command-line tools shipped by the wireguard-tools package.
package wg

import (
	"fmt"
	"os"
	"os/exec"
	"strconv"

	"github.com/evgnomon/portman/internal/config"
	"github.com/evgnomon/portman/internal/keys"
)

// Up brings up the WireGuard interface described by cfg: it creates the
// link (if missing), assigns the address, loads the private key and peers,
// and sets the link up. It is idempotent; calling it on an already-up
// interface reapplies the configuration.
func Up(cfg *config.Config) error {
	iface := cfg.Interface.Name

	if !linkExists(iface) {
		if err := run("ip", "link", "add", "dev", iface, "type", "wireguard"); err != nil {
			return fmt.Errorf("create interface %s: %w", iface, err)
		}
	}

	if err := configureDevice(cfg); err != nil {
		return err
	}

	if err := run("ip", "address", "replace", cfg.Interface.Address, "dev", iface); err != nil {
		return fmt.Errorf("assign address: %w", err)
	}

	if cfg.Interface.MTU > 0 {
		if err := run("ip", "link", "set", "dev", iface, "mtu", strconv.Itoa(cfg.Interface.MTU)); err != nil {
			return fmt.Errorf("set mtu: %w", err)
		}
	}

	if err := run("ip", "link", "set", "dev", iface, "up"); err != nil {
		return fmt.Errorf("set interface up: %w", err)
	}

	return nil
}

// Down removes the WireGuard interface. It is a no-op if the interface
// does not exist.
func Down(iface string) error {
	if !linkExists(iface) {
		return nil
	}
	if err := run("ip", "link", "del", "dev", iface); err != nil {
		return fmt.Errorf("delete interface %s: %w", iface, err)
	}
	return nil
}

// Status returns the output of `wg show <iface>`.
func Status(iface string) (string, error) {
	out, err := exec.Command("wg", "show", iface).CombinedOutput()
	if err != nil {
		return string(out), fmt.Errorf("wg show %s: %w", iface, err)
	}
	return string(out), nil
}

func configureDevice(cfg *config.Config) error {
	priv, err := keys.EnsurePrivateKey()
	if err != nil {
		return fmt.Errorf("load private key: %w", err)
	}

	keyFile, err := writeTempKey(priv)
	if err != nil {
		return err
	}
	defer os.Remove(keyFile)

	args := []string{
		"set", cfg.Interface.Name,
		"private-key", keyFile,
	}
	if cfg.Interface.ListenPort > 0 {
		args = append(args, "listen-port", strconv.Itoa(cfg.Interface.ListenPort))
	}

	for _, p := range cfg.Peers {
		args = append(args, "peer", p.PublicKey)

		if p.PresharedKey != "" {
			pskFile, err := writeTempKey(p.PresharedKey)
			if err != nil {
				return err
			}
			defer os.Remove(pskFile)
			args = append(args, "preshared-key", pskFile)
		}

		if len(p.AllowedIPs) > 0 {
			args = append(args, "allowed-ips", joinComma(p.AllowedIPs))
		}
		if p.Endpoint != "" {
			args = append(args, "endpoint", p.Endpoint)
		}
		if p.PersistentKeepalive > 0 {
			args = append(args, "persistent-keepalive", strconv.Itoa(p.PersistentKeepalive))
		}
	}

	if err := run("wg", args...); err != nil {
		return fmt.Errorf("configure wireguard device: %w", err)
	}
	return nil
}

// writeTempKey writes key material to a 0600 file owned by the current
// process so wg(8) can read it without the secret ever appearing in the
// process argument list (visible via ps/proc to other users).
func writeTempKey(key string) (string, error) {
	f, err := os.CreateTemp("", "portman-key-*")
	if err != nil {
		return "", fmt.Errorf("create temp key file: %w", err)
	}
	name := f.Name()
	if err := f.Chmod(0o600); err != nil {
		f.Close()
		os.Remove(name)
		return "", fmt.Errorf("chmod temp key file: %w", err)
	}
	if _, err := f.WriteString(key + "\n"); err != nil {
		f.Close()
		os.Remove(name)
		return "", fmt.Errorf("write temp key file: %w", err)
	}
	if err := f.Close(); err != nil {
		os.Remove(name)
		return "", fmt.Errorf("close temp key file: %w", err)
	}
	return name, nil
}

func linkExists(iface string) bool {
	return exec.Command("ip", "link", "show", "dev", iface).Run() == nil
}

func run(name string, args ...string) error {
	cmd := exec.Command(name, args...)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("%s %v: %w: %s", name, args, err, string(out))
	}
	return nil
}

func joinComma(items []string) string {
	out := ""
	for i, s := range items {
		if i > 0 {
			out += ","
		}
		out += s
	}
	return out
}

// Package app orchestrates the WireGuard interface and, on any host that
// has forward rules, the DNAT rules that carry traffic into the mesh.
package app

import (
	"fmt"
	"os"

	"github.com/evgnomon/portman/internal/config"
	"github.com/evgnomon/portman/internal/iptables"
	"github.com/evgnomon/portman/internal/wg"
)

// Up brings the tunnel up, and applies DNAT/forwarding rules if cfg has
// any forward-rule services (config.Config.Forwards).
func Up(cfg *config.Config) error {
	if err := wg.Up(cfg); err != nil {
		return err
	}

	if len(cfg.Forwards()) > 0 {
		if err := enableIPForwarding(); err != nil {
			return err
		}
		if err := iptables.Apply(cfg); err != nil {
			return err
		}
	} else {
		// Nothing to forward (any more) - make sure a stale ruleset from
		// a previous config isn't left behind.
		if err := iptables.Flush(); err != nil {
			return err
		}
	}

	return nil
}

// Down tears down the DNAT rules (if any) and the WireGuard interface.
func Down(cfg *config.Config) error {
	if err := iptables.Flush(); err != nil {
		return err
	}
	return wg.Down(cfg.Interface.Name)
}

// Reload reapplies the WireGuard peer/service configuration without
// tearing down the interface, e.g. after `portman import`/`forward`.
func Reload(cfg *config.Config) error {
	return Up(cfg)
}

// Status prints the WireGuard link state and, if this host has forward
// rules, the active DNAT ruleset.
func Status(cfg *config.Config) (string, error) {
	out, err := wg.Status(cfg.Interface.Name)
	if err != nil {
		return out, err
	}

	if len(cfg.Forwards()) > 0 {
		rules, rerr := iptables.Ruleset()
		if rerr == nil {
			out += "\n" + rules
		}
	}

	return out, nil
}

// enableIPForwarding turns on net.ipv4.ip_forward, which is required to
// route DNAT'd traffic on to WireGuard peers.
func enableIPForwarding() error {
	const path = "/proc/sys/net/ipv4/ip_forward"
	if err := os.WriteFile(path, []byte("1\n"), 0o644); err != nil {
		return fmt.Errorf("enable ip forwarding (%s): %w", path, err)
	}
	return nil
}

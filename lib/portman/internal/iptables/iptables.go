// Package iptables manages the DNAT/forwarding rules for the [[service]]
// entries that are forward rules (config.Service.IsForward). All rules
// live in three dedicated chains, jumped to from the built-in ones, so
// they can be applied and torn down without touching any other firewall
// rules on the host.
package iptables

import (
	"fmt"
	"net"
	"os/exec"
	"strconv"
	"strings"

	"github.com/evgnomon/portman/internal/config"
)

// Chain names portman owns exclusively, one per hook it needs.
const (
	preChain  = "PORTMAN_PRE"  // nat/PREROUTING: DNAT forward rules
	postChain = "PORTMAN_POST" // nat/POSTROUTING: masquerade
	fwdChain  = "PORTMAN_FWD"  // filter/FORWARD: accept tunnel traffic
)

// jump is a (table, builtin chain, owned chain) hookpoint.
type jump struct {
	table, builtin, owned string
}

var jumps = []jump{
	{"nat", "PREROUTING", preChain},
	{"nat", "POSTROUTING", postChain},
	{"filter", "FORWARD", fwdChain},
}

// Apply (re)builds portman's chains from cfg. Safe to call repeatedly; it
// replaces any previous portman ruleset atomically-enough (flush, then
// rebuild) for a rarely-changed forwarding table.
func Apply(cfg *config.Config) error {
	if err := Flush(); err != nil {
		return err
	}

	for _, j := range jumps {
		if err := run("iptables", "-t", j.table, "-N", j.owned); err != nil {
			return fmt.Errorf("create chain %s: %w", j.owned, err)
		}
		if err := run("iptables", "-t", j.table, "-A", j.builtin, "-j", j.owned); err != nil {
			return fmt.Errorf("hook %s into %s/%s: %w", j.owned, j.table, j.builtin, err)
		}
	}

	for _, s := range cfg.Forwards() {
		rule, err := preroute(s)
		if err != nil {
			return fmt.Errorf("service %q: %w", s.Name, err)
		}
		if err := run("iptables", rule...); err != nil {
			return fmt.Errorf("service %q: apply DNAT rule: %w", s.Name, err)
		}
	}

	if err := run("iptables", "-t", "nat", "-A", postChain, "-o", cfg.Interface.Name, "-j", "MASQUERADE"); err != nil {
		return fmt.Errorf("masquerade: %w", err)
	}
	if err := run("iptables", "-A", fwdChain, "-i", cfg.Interface.Name, "-j", "ACCEPT"); err != nil {
		return fmt.Errorf("forward accept (in): %w", err)
	}
	if err := run("iptables", "-A", fwdChain, "-o", cfg.Interface.Name, "-j", "ACCEPT"); err != nil {
		return fmt.Errorf("forward accept (out): %w", err)
	}

	return nil
}

// Flush removes portman's jump rules and chains, if present. It is a
// no-op (not an error) if nothing was ever applied.
func Flush() error {
	for _, j := range jumps {
		_ = run("iptables", "-t", j.table, "-D", j.builtin, "-j", j.owned) // may not exist
		_ = run("iptables", "-t", j.table, "-F", j.owned)                 // must be empty before -X
		_ = run("iptables", "-t", j.table, "-X", j.owned)
	}
	return nil
}

// Ruleset returns portman's chains as text, for `portman status`.
func Ruleset() (string, error) {
	var b strings.Builder
	for _, j := range jumps {
		out, err := exec.Command("iptables", "-t", j.table, "-L", j.owned, "-n", "-v").CombinedOutput()
		if err != nil {
			continue // chain doesn't exist yet - nothing forwarded
		}
		b.Write(out)
		b.WriteString("\n")
	}
	if b.Len() == 0 {
		return "", fmt.Errorf("no portman iptables chains found")
	}
	return b.String(), nil
}

func preroute(s config.Service) ([]string, error) {
	host, port, err := net.SplitHostPort(s.RemoteBind)
	if err != nil {
		return nil, fmt.Errorf("remote_bind %q: %w", s.RemoteBind, err)
	}

	args := []string{"-t", "nat", "-A", preChain, "-p", s.ProtocolOrDefault()}
	if host != "" && host != "0.0.0.0" && host != "*" {
		args = append(args, "-d", host)
	}
	args = append(args, "--dport", port,
		"-j", "DNAT", "--to-destination", net.JoinHostPort(s.ClientAddress, strconv.Itoa(s.LocalPort)),
		"-m", "comment", "--comment", "portman:"+s.Name,
	)
	return args, nil
}

func run(name string, args ...string) error {
	cmd := exec.Command(name, args...)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("%s %v: %w: %s", name, args, err, strings.TrimSpace(string(out)))
	}
	return nil
}

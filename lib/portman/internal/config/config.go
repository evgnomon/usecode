// Package config loads, validates, and mutates the portman configuration
// file. There is no client/server "mode": every host runs the same
// commands, and a host's role falls out of what its config contains - a
// [[service]] with only local_port is something this host runs; a
// [[service]] with remote_bind+client_address is something this host
// forwards to a peer.
package config

import (
	"fmt"
	"net"
	"os"
	"path/filepath"
	"strings"

	"github.com/BurntSushi/toml"
)

// DefaultPath is the well-known location for the portman config file.
const DefaultPath = "/etc/portman/config.toml"

// Interface holds the local WireGuard interface settings. The private
// key is deliberately not part of the config file: portman generates
// and persists it itself (see internal/keys) so it never has to be
// typed, pasted, or committed anywhere.
type Interface struct {
	Name       string `toml:"interface"`
	Address    string `toml:"address"`
	ListenPort int    `toml:"listen_port"`
	MTU        int    `toml:"mtu"`
}

// Peer describes a remote WireGuard peer. Peers are normally added with
// `portman import`, not hand-edited.
type Peer struct {
	Name                string   `toml:"name"`
	PublicKey           string   `toml:"public_key"`
	PresharedKey        string   `toml:"preshared_key,omitempty"`
	Endpoint            string   `toml:"endpoint,omitempty"`
	AllowedIPs          []string `toml:"allowed_ips"`
	PersistentKeepalive int      `toml:"persistent_keepalive,omitempty"`
}

// Service is either a local declaration ("I run this on local_port") or a
// forward rule ("public traffic on remote_bind goes to client_address:
// local_port"), set with `portman forward`/`portman unforward`. Which one
// it is follows from whether RemoteBind is set - there is no separate
// flag for it.
type Service struct {
	Name          string `toml:"name"`
	Protocol      string `toml:"protocol"`                 // "tcp" or "udp", default "tcp"
	RemoteBind    string `toml:"remote_bind,omitempty"`    // e.g. "0.0.0.0:443" (only set on a forward rule)
	ClientAddress string `toml:"client_address,omitempty"` // WireGuard address of the peer being forwarded to
	LocalPort     int    `toml:"local_port"`               // the port this rule ultimately targets
}

// IsForward reports whether s is a forward rule (public bind -> a peer's
// port) rather than a plain local service declaration.
func (s Service) IsForward() bool {
	return s.RemoteBind != ""
}

// Config is the root portman configuration.
type Config struct {
	Interface Interface `toml:"wireguard"`
	Peers     []Peer    `toml:"peer"`
	Services  []Service `toml:"service"`
}

// Descriptor is the small, non-secret bundle a host hands to another host
// so it can be added as a peer, via `portman export` / `portman import`.
// It never contains a private key.
type Descriptor struct {
	Name      string `toml:"name"`
	PublicKey string `toml:"public_key"`
	Address   string `toml:"address"` // this host's bare WireGuard IP (no prefix length)
	Endpoint  string `toml:"endpoint,omitempty"`
}

// Load reads and validates the config file at path. If no file exists at
// path, it writes a commented-out template there (root-only) and returns
// an error asking the caller to run `portman export` first, rather than
// failing with a bare "no such file" error. It refuses to load a file that
// is readable or writable by anyone other than its owner, since it may
// contain WireGuard preshared keys.
func Load(path string) (*Config, error) {
	created, err := ensureDefault(path)
	if err != nil {
		return nil, err
	}
	if created {
		return nil, fmt.Errorf("no config found; wrote a template to %s - run `portman export` to fill in this host's identity, then `portman import` to add peers", path)
	}

	if err := checkPermissions(path); err != nil {
		return nil, err
	}

	var cfg Config
	if _, err := toml.DecodeFile(path, &cfg); err != nil {
		return nil, fmt.Errorf("parse %s: %w", path, err)
	}

	if err := cfg.Validate(); err != nil {
		return nil, err
	}

	return &cfg, nil
}

// New creates a minimal config with just the wireguard interface section
// (no peers, no services) and saves it to path. It fails if a config
// already exists at path - use Load in that case. This is what
// `portman export` calls the first time it runs on a host.
func New(path, iface, address string, listenPort int) (*Config, error) {
	if _, err := os.Stat(path); err == nil {
		return nil, fmt.Errorf("%s already exists", path)
	} else if !os.IsNotExist(err) {
		return nil, fmt.Errorf("stat %s: %w", path, err)
	}

	cfg := &Config{Interface: Interface{Name: iface, Address: address, ListenPort: listenPort}}
	if _, _, err := net.ParseCIDR(address); err != nil {
		return nil, fmt.Errorf("address %q must be a CIDR (e.g. 10.10.0.2/24): %w", address, err)
	}
	if err := cfg.Save(path); err != nil {
		return nil, err
	}
	return cfg, nil
}

// Save validates and writes the config back to path, root-only. Hand
// edits/comments in an existing file at path are lost once this is
// called - after the first `portman export`/`import`/`forward`, the
// config file is owned by portman's own commands.
func (c *Config) Save(path string) error {
	if err := c.Validate(); err != nil {
		return err
	}

	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return fmt.Errorf("create %s: %w", filepath.Dir(path), err)
	}

	tmp := path + ".tmp"
	f, err := os.OpenFile(tmp, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, 0o600)
	if err != nil {
		return fmt.Errorf("write %s: %w", path, err)
	}
	if err := toml.NewEncoder(f).Encode(c); err != nil {
		f.Close()
		os.Remove(tmp)
		return fmt.Errorf("encode %s: %w", path, err)
	}
	if err := f.Close(); err != nil {
		os.Remove(tmp)
		return fmt.Errorf("write %s: %w", path, err)
	}
	if err := os.Rename(tmp, path); err != nil {
		return fmt.Errorf("write %s: %w", path, err)
	}
	return nil
}

// AddOrReplacePeer adds p, or replaces the existing peer with the same
// Name if one exists (re-importing a descriptor updates it in place
// instead of duplicating it).
func (c *Config) AddOrReplacePeer(p Peer) {
	for i, existing := range c.Peers {
		if existing.Name == p.Name {
			c.Peers[i] = p
			return
		}
	}
	c.Peers = append(c.Peers, p)
}

// AddOrReplaceService adds s, or replaces the existing service with the
// same Name if one exists (`portman forward` on an existing name repoints
// it instead of duplicating it).
func (c *Config) AddOrReplaceService(s Service) {
	for i, existing := range c.Services {
		if existing.Name == s.Name {
			c.Services[i] = s
			return
		}
	}
	c.Services = append(c.Services, s)
}

// RemoveService deletes the service named name. It reports whether
// anything was removed.
func (c *Config) RemoveService(name string) bool {
	for i, s := range c.Services {
		if s.Name == name {
			c.Services = append(c.Services[:i], c.Services[i+1:]...)
			return true
		}
	}
	return false
}

// PeerByName looks up a peer by name.
func (c *Config) PeerByName(name string) (Peer, bool) {
	for _, p := range c.Peers {
		if p.Name == name {
			return p, true
		}
	}
	return Peer{}, false
}

// Forwards returns the services that are forward rules (RemoteBind set).
// A non-empty result is what makes this host run DNAT/IP-forwarding.
func (c *Config) Forwards() []Service {
	var out []Service
	for _, s := range c.Services {
		if s.IsForward() {
			out = append(out, s)
		}
	}
	return out
}

// Address returns this host's bare WireGuard IP (the CIDR in
// wireguard.address without its prefix length).
func (c *Config) Address() (string, error) {
	ip, _, err := net.ParseCIDR(c.Interface.Address)
	if err != nil {
		return "", fmt.Errorf("wireguard.address %q must be a CIDR: %w", c.Interface.Address, err)
	}
	return ip.String(), nil
}

// defaultTemplate is written to path when no config file exists yet. It
// is a starting point, not a working config.
const defaultTemplate = `# portman config - run "portman export" to fill this in, then "portman
# import" to add peers and "portman forward" to declare services. See
# README.md for a worked recipe.

[wireguard]
interface = "wg-portman"
address   = "10.10.0.2/24" # this host's WireGuard address (CIDR)
`

// ensureDefault writes defaultTemplate to path, root-only, if no file
// exists there yet. It reports whether it created the file.
func ensureDefault(path string) (bool, error) {
	if _, err := os.Stat(path); err == nil {
		return false, nil
	} else if !os.IsNotExist(err) {
		return false, fmt.Errorf("stat %s: %w", path, err)
	}

	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return false, fmt.Errorf("create %s: %w", filepath.Dir(path), err)
	}
	if err := os.WriteFile(path, []byte(defaultTemplate), 0o600); err != nil {
		return false, fmt.Errorf("write %s: %w", path, err)
	}

	return true, nil
}

// checkPermissions rejects config files that are group- or world-readable,
// since they contain secret key material that must stay root-only.
func checkPermissions(path string) error {
	info, err := os.Stat(path)
	if err != nil {
		return fmt.Errorf("stat %s: %w", path, err)
	}

	if info.Mode().Perm()&0o077 != 0 {
		return fmt.Errorf(
			"refusing to load %s: mode %04o is accessible to group/other; run `chmod 600 %s`",
			path, info.Mode().Perm(), path,
		)
	}

	return nil
}

// Validate checks that the config is internally consistent. There is no
// mode to check against: every field is validated on its own terms, and
// a config with zero peers or zero services is valid (a freshly exported
// host that hasn't imported or forwarded anything yet).
func (c *Config) Validate() error {
	if c.Interface.Name == "" {
		return fmt.Errorf("wireguard.interface is required")
	}
	if c.Interface.Address == "" {
		return fmt.Errorf("wireguard.address is required")
	}
	if _, _, err := net.ParseCIDR(c.Interface.Address); err != nil {
		return fmt.Errorf("wireguard.address %q must be a CIDR (e.g. 10.10.0.2/24): %w", c.Interface.Address, err)
	}

	var errs []error
	for i, p := range c.Peers {
		if p.Name == "" {
			errs = append(errs, fmt.Errorf("peer[%d]: name is required", i))
		}
		if p.PublicKey == "" {
			errs = append(errs, fmt.Errorf("peer[%d]: public_key is required", i))
		}
		if len(p.AllowedIPs) == 0 {
			errs = append(errs, fmt.Errorf("peer[%d]: allowed_ips is required", i))
		}
	}

	for i, s := range c.Services {
		if s.Name == "" {
			errs = append(errs, fmt.Errorf("service[%d]: name is required", i))
		}
		switch s.Protocol {
		case "", "tcp", "udp":
		default:
			errs = append(errs, fmt.Errorf("service[%d]: protocol must be \"tcp\" or \"udp\"", i))
		}
		if s.LocalPort == 0 {
			errs = append(errs, fmt.Errorf("service[%d]: local_port is required", i))
		}
		hasBind, hasAddr := s.RemoteBind != "", s.ClientAddress != ""
		if hasBind != hasAddr {
			errs = append(errs, fmt.Errorf("service[%d]: remote_bind and client_address must be set together (it's a forward rule) or both empty (it's a local declaration)", i))
		}
		if hasBind {
			if _, _, err := net.SplitHostPort(s.RemoteBind); err != nil {
				errs = append(errs, fmt.Errorf("service[%d]: remote_bind %q must be host:port: %w", i, s.RemoteBind, err))
			}
		}
		if hasAddr && net.ParseIP(s.ClientAddress) == nil {
			errs = append(errs, fmt.Errorf("service[%d]: client_address %q is not a valid IP", i, s.ClientAddress))
		}
	}

	return joinErrs(errs)
}

func joinErrs(errs []error) error {
	if len(errs) == 0 {
		return nil
	}
	var b strings.Builder
	fmt.Fprintf(&b, "%d config error(s):", len(errs))
	for _, e := range errs {
		fmt.Fprintf(&b, "\n  - %s", e)
	}
	return fmt.Errorf("%s", b.String())
}

// Protocol returns the service's protocol, defaulting to tcp.
func (s Service) ProtocolOrDefault() string {
	if s.Protocol == "" {
		return "tcp"
	}
	return s.Protocol
}

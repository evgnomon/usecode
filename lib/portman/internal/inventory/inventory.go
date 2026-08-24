// Package inventory reads and extends the mesh topology, which lives in
// one place: a multi-file Ansible inventory under deploy/inventory.
//
// The topology is the single source of truth for who is in the mesh and
// what address each host holds. Nothing allocates an address on the host
// itself any more, which is what stops two hosts from ever being handed
// the same one: `portman add` looks at every address already recorded
// here and picks the lowest free one in the configured network.
//
// The layout is:
//
//	deploy/inventory/hosts.yml                       group membership
//	deploy/inventory/group_vars/portman/main.yml     mesh-wide inputs
//	deploy/inventory/group_vars/portman/secrets.yml  vaulted private keys
//	deploy/inventory/host_vars/<host>.yml            one host's unique facts
//
// Only hosts.yml and host_vars/<host>.yml are written by portman, and
// only ever by adding to them: a host file is created once, when the
// host joins, and is yours to hand-edit afterwards.
package inventory

import (
	"bytes"
	"fmt"
	"net/netip"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"gopkg.in/yaml.v3"
)

// Group is the inventory group whose members portman manages.
const Group = "portman"

// DefaultDir is where the inventory lives inside a portman checkout,
// relative to the repository root.
const DefaultDir = "deploy/inventory"

// Settings are the mesh-wide inputs, read from
// group_vars/portman/main.yml. These are the knobs a human sets; every
// per-host value is derived from them.
type Settings struct {
	// Network is the CIDR every host's tunnel address is drawn from.
	Network string `yaml:"portman_network"`
	// AddressStart is the first host number in Network that may be
	// allocated, so that e.g. .1 can be kept for a router if wanted.
	AddressStart int `yaml:"portman_address_start"`
	// ListenPort is the WireGuard port, used to build the endpoint of a
	// host added with a public address but no explicit port.
	ListenPort int `yaml:"portman_listen_port"`
}

// Host is one mesh member's unique facts, stored in
// host_vars/<name>.yml. Fields portman does not manage (extra Ansible
// vars, services added by hand) are preserved because portman only ever
// creates this file, never rewrites it.
type Host struct {
	Name string `yaml:"-"`

	AnsibleHost string `yaml:"ansible_host,omitempty"`
	AnsibleUser string `yaml:"ansible_user,omitempty"`

	// Address is this host's tunnel address without a prefix length,
	// e.g. "10.10.0.3". The prefix comes from Settings.Network.
	Address string `yaml:"portman_address"`
	// PublicKey is the WireGuard public key of the keypair minted for
	// this host; its private half lives in the vault, never here.
	PublicKey string `yaml:"portman_public_key"`
	// Endpoint is host:port other members should dial to reach this
	// host. Empty means this host only dials out (it is behind NAT).
	Endpoint string `yaml:"portman_endpoint,omitempty"`

	// Services are this host's [[service]] declarations, filled in by
	// hand (or with `portman forward` on the host itself).
	Services []map[string]any `yaml:"portman_services"`
}

// Inventory is the topology as read off disk.
type Inventory struct {
	// Dir is the inventory root, e.g. <repo>/deploy/inventory.
	Dir string
	// Root is the directory ansible commands should run from - the repo
	// root, two levels above Dir, which is where ansible.cfg lives.
	Root string

	Settings Settings
	Hosts    []Host
}

// Paths within the inventory.
func (inv *Inventory) hostsPath() string { return filepath.Join(inv.Dir, "hosts.yml") }
func (inv *Inventory) settingsPath() string {
	return filepath.Join(inv.Dir, "group_vars", Group, "main.yml")
}
func (inv *Inventory) hostVarsPath(name string) string {
	return filepath.Join(inv.Dir, "host_vars", name+".yml")
}

// SecretsPath is the vaulted file holding every host's private key.
func (inv *Inventory) SecretsPath() string {
	return filepath.Join(inv.Dir, "group_vars", Group, "secrets.yml")
}

// Load reads the inventory rooted at dir.
func Load(dir string) (*Inventory, error) {
	abs, err := filepath.Abs(dir)
	if err != nil {
		return nil, fmt.Errorf("resolve %s: %w", dir, err)
	}
	inv := &Inventory{Dir: abs, Root: filepath.Dir(filepath.Dir(abs))}

	if _, err := os.Stat(inv.hostsPath()); err != nil {
		return nil, fmt.Errorf("no inventory at %s (expected %s); pass -inventory DIR", dir, inv.hostsPath())
	}

	settings, err := os.ReadFile(inv.settingsPath())
	if err != nil {
		return nil, fmt.Errorf("read %s: %w", inv.settingsPath(), err)
	}
	if err := yaml.Unmarshal(settings, &inv.Settings); err != nil {
		return nil, fmt.Errorf("parse %s: %w", inv.settingsPath(), err)
	}
	if inv.Settings.Network == "" {
		return nil, fmt.Errorf("%s: portman_network is required", inv.settingsPath())
	}

	names, err := inv.hostNames()
	if err != nil {
		return nil, err
	}
	for _, name := range names {
		host, err := inv.loadHost(name)
		if err != nil {
			return nil, err
		}
		inv.Hosts = append(inv.Hosts, host)
	}

	return inv, nil
}

// hostNames returns the members of the portman group in hosts.yml.
func (inv *Inventory) hostNames() ([]string, error) {
	data, err := os.ReadFile(inv.hostsPath())
	if err != nil {
		return nil, fmt.Errorf("read %s: %w", inv.hostsPath(), err)
	}

	// Only the portman group's host keys are needed, so decode into a
	// loose map rather than modelling Ansible's whole inventory schema.
	var doc struct {
		All struct {
			Children map[string]struct {
				Hosts map[string]any `yaml:"hosts"`
			} `yaml:"children"`
		} `yaml:"all"`
	}
	if err := yaml.Unmarshal(data, &doc); err != nil {
		return nil, fmt.Errorf("parse %s: %w", inv.hostsPath(), err)
	}

	names := make([]string, 0, len(doc.All.Children[Group].Hosts))
	for name := range doc.All.Children[Group].Hosts {
		names = append(names, name)
	}
	sort.Strings(names)
	return names, nil
}

func (inv *Inventory) loadHost(name string) (Host, error) {
	path := inv.hostVarsPath(name)
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return Host{}, fmt.Errorf("%s is in the %s group but %s is missing; add it or remove the host from %s",
				name, Group, path, inv.hostsPath())
		}
		return Host{}, fmt.Errorf("read %s: %w", path, err)
	}

	host := Host{Name: name}
	if err := yaml.Unmarshal(data, &host); err != nil {
		return Host{}, fmt.Errorf("parse %s: %w", path, err)
	}
	if host.Address == "" {
		return Host{}, fmt.Errorf("%s: portman_address is required", path)
	}
	return host, nil
}

// Host looks up a member by inventory hostname.
func (inv *Inventory) Host(name string) (Host, bool) {
	for _, h := range inv.Hosts {
		if h.Name == name {
			return h, true
		}
	}
	return Host{}, false
}

// NewHost is the request to put a host into the mesh. Every field is
// optional except Name; Address is allocated when left empty.
type NewHost struct {
	Name        string
	Address     string
	Endpoint    string
	AnsibleHost string
	AnsibleUser string
}

// Add records a new mesh member: it allocates the host's tunnel address
// (unless one was given), mints its WireGuard keypair, writes
// host_vars/<name>.yml, appends the host to the portman group in
// hosts.yml, and stores the private key in the vault. It returns the
// host as recorded.
//
// It is refused if the host is already in the inventory: re-adding would
// mean minting a second keypair for a host that already has one, which
// silently breaks its tunnel. Edit host_vars/<name>.yml instead.
func (inv *Inventory) Add(req NewHost, vault *Vault) (Host, error) {
	if err := validName(req.Name); err != nil {
		return Host{}, err
	}
	if _, exists := inv.Host(req.Name); exists {
		return Host{}, fmt.Errorf("%s is already in the mesh (see %s)", req.Name, inv.hostVarsPath(req.Name))
	}
	if _, err := os.Stat(inv.hostVarsPath(req.Name)); err == nil {
		return Host{}, fmt.Errorf("%s already exists but %s is not in the %s group; "+
			"add it there or delete the file", inv.hostVarsPath(req.Name), req.Name, Group)
	}

	address := req.Address
	if address == "" {
		allocated, err := inv.allocate()
		if err != nil {
			return Host{}, err
		}
		address = allocated
	} else if err := inv.checkAddress(address); err != nil {
		return Host{}, err
	}

	priv, pub, err := newKeypair()
	if err != nil {
		return Host{}, err
	}

	host := Host{
		Name:        req.Name,
		AnsibleHost: req.AnsibleHost,
		AnsibleUser: req.AnsibleUser,
		Address:     address,
		PublicKey:   pub,
		Endpoint:    inv.endpoint(req.Endpoint),
		Services:    []map[string]any{},
	}

	// The private key goes in first: a vault write that fails (wrong
	// password, no ansible-vault) leaves the inventory untouched rather
	// than leaving a host with no key behind.
	if err := vault.Put(host.Name, priv); err != nil {
		return Host{}, err
	}
	if err := inv.writeHostVars(host); err != nil {
		return Host{}, err
	}
	if err := inv.addToGroup(host.Name); err != nil {
		return Host{}, err
	}

	inv.Hosts = append(inv.Hosts, host)
	sort.Slice(inv.Hosts, func(i, j int) bool { return inv.Hosts[i].Name < inv.Hosts[j].Name })
	return host, nil
}

// endpoint completes a dialable address: given just a host or IP, it
// appends the mesh's WireGuard port, since that is the only port an
// endpoint could mean.
func (inv *Inventory) endpoint(value string) string {
	if value == "" || strings.Contains(value, ":") {
		return value
	}
	port := inv.Settings.ListenPort
	if port == 0 {
		port = 51820
	}
	return fmt.Sprintf("%s:%d", value, port)
}

// allocate returns the lowest address in the configured network that no
// host holds yet. This is the whole point of a central topology: the
// answer depends on every other host, so it cannot be decided on the
// host being added.
func (inv *Inventory) allocate() (string, error) {
	prefix, err := inv.network()
	if err != nil {
		return "", err
	}

	taken := make(map[netip.Addr]string, len(inv.Hosts))
	for _, h := range inv.Hosts {
		addr, err := netip.ParseAddr(h.Address)
		if err != nil {
			return "", fmt.Errorf("%s: portman_address %q is not an IP address: %w", h.Name, h.Address, err)
		}
		if other, clash := taken[addr]; clash {
			return "", fmt.Errorf("%s and %s both claim %s; fix the topology before adding hosts", other, h.Name, addr)
		}
		taken[addr] = h.Name
	}

	for addr := hostNumber(prefix, inv.Settings.AddressStart); prefix.Contains(addr); addr = addr.Next() {
		if isBroadcast(prefix, addr) {
			break
		}
		if _, used := taken[addr]; !used {
			return addr.String(), nil
		}
	}

	return "", fmt.Errorf("no free address left in portman_network %s (%d hosts); widen the network in %s",
		inv.Settings.Network, len(inv.Hosts), inv.settingsPath())
}

// checkAddress validates an operator-supplied address against the same
// rules allocation follows, so -address can't reintroduce a clash.
func (inv *Inventory) checkAddress(address string) error {
	addr, err := netip.ParseAddr(address)
	if err != nil {
		return fmt.Errorf("-address %q is not an IP address (it takes a bare address, e.g. 10.10.0.5): %w", address, err)
	}
	prefix, err := inv.network()
	if err != nil {
		return err
	}
	if !prefix.Contains(addr) {
		return fmt.Errorf("-address %s is outside portman_network %s", addr, inv.Settings.Network)
	}
	for _, h := range inv.Hosts {
		if h.Address == addr.String() {
			return fmt.Errorf("-address %s is already held by %s", addr, h.Name)
		}
	}
	return nil
}

func (inv *Inventory) network() (netip.Prefix, error) {
	prefix, err := netip.ParsePrefix(inv.Settings.Network)
	if err != nil {
		return netip.Prefix{}, fmt.Errorf("%s: portman_network %q must be a CIDR (e.g. 10.10.0.0/24): %w",
			inv.settingsPath(), inv.Settings.Network, err)
	}
	if !prefix.Addr().Is4() {
		return netip.Prefix{}, fmt.Errorf("%s: portman_network %q must be IPv4", inv.settingsPath(), inv.Settings.Network)
	}
	return prefix.Masked(), nil
}

// Prefix returns the network's prefix length, which is what turns a
// host's bare address into the CIDR WireGuard wants.
func (inv *Inventory) Prefix() (int, error) {
	prefix, err := inv.network()
	if err != nil {
		return 0, err
	}
	return prefix.Bits(), nil
}

// hostNumber returns the nth address in prefix, counting the network
// address as 0.
func hostNumber(prefix netip.Prefix, n int) netip.Addr {
	addr := prefix.Addr()
	if n < 1 {
		n = 1
	}
	for i := 0; i < n; i++ {
		addr = addr.Next()
	}
	return addr
}

// isBroadcast reports whether addr is the all-ones address of prefix,
// which is not usable by a host.
func isBroadcast(prefix netip.Prefix, addr netip.Addr) bool {
	if prefix.Bits() >= 31 {
		return false
	}
	b := addr.As4()
	hostBits := 32 - prefix.Bits()
	for i := 0; i < hostBits; i++ {
		if b[3-i/8]&(1<<(i%8)) == 0 {
			return false
		}
	}
	return true
}

func (inv *Inventory) writeHostVars(host Host) error {
	body, err := yaml.Marshal(host)
	if err != nil {
		return fmt.Errorf("encode host vars for %s: %w", host.Name, err)
	}

	header := fmt.Sprintf(`---
# %s - one member of the portman mesh.
#
# Created by `+"`portman add %s`"+`, and not touched by portman again:
# edit it freely. portman_address was allocated from portman_network in
# group_vars/%s/main.yml, and the private key half of
# portman_public_key is in group_vars/%s/secrets.yml under this host's
# name.
`, host.Name, host.Name, Group, Group)

	path := inv.hostVarsPath(host.Name)
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("create %s: %w", filepath.Dir(path), err)
	}
	if err := os.WriteFile(path, append([]byte(header), body...), 0o644); err != nil {
		return fmt.Errorf("write %s: %w", path, err)
	}
	return nil
}

// addToGroup appends name to the portman group in hosts.yml, editing the
// parsed document tree rather than re-encoding it, so the comments and
// layout of a hand-maintained inventory survive.
func (inv *Inventory) addToGroup(name string) error {
	path := inv.hostsPath()
	data, err := os.ReadFile(path)
	if err != nil {
		return fmt.Errorf("read %s: %w", path, err)
	}

	var doc yaml.Node
	if err := yaml.Unmarshal(data, &doc); err != nil {
		return fmt.Errorf("parse %s: %w", path, err)
	}
	if len(doc.Content) == 0 {
		return fmt.Errorf("%s is empty", path)
	}

	hosts, err := groupHostsNode(doc.Content[0])
	if err != nil {
		return fmt.Errorf("%s: %w", path, err)
	}
	hosts.Content = append(hosts.Content,
		&yaml.Node{Kind: yaml.ScalarNode, Tag: "!!str", Value: name},
		&yaml.Node{Kind: yaml.MappingNode, Tag: "!!map", Style: yaml.FlowStyle},
	)

	out, err := marshalDoc(&doc)
	if err != nil {
		return fmt.Errorf("encode %s: %w", path, err)
	}
	// The encoder drops an explicit document start; keep the file
	// looking exactly like the one that was read.
	if bytes.HasPrefix(data, []byte("---\n")) && !bytes.HasPrefix(out, []byte("---\n")) {
		out = append([]byte("---\n"), out...)
	}
	if err := os.WriteFile(path, out, 0o644); err != nil {
		return fmt.Errorf("write %s: %w", path, err)
	}
	return nil
}

// groupHostsNode walks all -> children -> portman -> hosts, creating any
// level that doesn't exist yet, and returns the hosts mapping node.
func groupHostsNode(root *yaml.Node) (*yaml.Node, error) {
	all, err := mappingChild(root, "all")
	if err != nil {
		return nil, err
	}
	children, err := mappingChild(all, "children")
	if err != nil {
		return nil, err
	}
	group, err := mappingChild(children, Group)
	if err != nil {
		return nil, err
	}
	return mappingChild(group, "hosts")
}

// mappingChild returns the mapping stored under key in node, adding an
// empty one if the key is absent or explicitly null (`portman:` with
// nothing under it, which is how an empty Ansible group is written).
func mappingChild(node *yaml.Node, key string) (*yaml.Node, error) {
	if node.Kind != yaml.MappingNode {
		return nil, fmt.Errorf("expected a mapping at %q", key)
	}
	for i := 0; i+1 < len(node.Content); i += 2 {
		if node.Content[i].Value != key {
			continue
		}
		value := node.Content[i+1]
		switch {
		case value.Kind == yaml.MappingNode:
			// An empty flow mapping ({}) would render the new entry
			// inline; make it a block so hosts stay one per line.
			value.Style = 0
			return value, nil
		case value.Tag == "!!null" || value.Kind == 0:
			*value = yaml.Node{Kind: yaml.MappingNode, Tag: "!!map"}
			return value, nil
		default:
			return nil, fmt.Errorf("%q is not a mapping", key)
		}
	}

	value := &yaml.Node{Kind: yaml.MappingNode, Tag: "!!map"}
	node.Content = append(node.Content,
		&yaml.Node{Kind: yaml.ScalarNode, Tag: "!!str", Value: key},
		value,
	)
	return value, nil
}

func validName(name string) error {
	if name == "" {
		return fmt.Errorf("a host name is required")
	}
	if strings.ContainsAny(name, "/\\ \t:") || name == "." || name == ".." {
		return fmt.Errorf("%q is not a usable inventory hostname", name)
	}
	return nil
}

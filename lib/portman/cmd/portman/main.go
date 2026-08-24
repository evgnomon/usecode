// Command portman sets up a WireGuard mesh and configures DNAT rules so
// that traffic to a public address lands on a port running elsewhere in
// the mesh. There is no client/server distinction at the command level:
// every host runs the same commands, and what a host actually does
// (accept inbound connections, forward traffic) follows from its config.
package main

import (
	"flag"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/BurntSushi/toml"

	"github.com/evgnomon/portman/internal/app"
	"github.com/evgnomon/portman/internal/config"
	"github.com/evgnomon/portman/internal/inventory"
	"github.com/evgnomon/portman/internal/keys"
)

var version = "dev"

func main() {
	if err := run(os.Args[1:]); err != nil {
		fmt.Fprintln(os.Stderr, "portman: error:", err)
		os.Exit(1)
	}
}

func run(args []string) error {
	if len(args) == 0 {
		usage()
		return nil
	}

	cmd, rest := args[0], args[1:]

	switch cmd {
	case "up":
		return withConfig(rest, requireRoot, app.Up)
	case "down":
		return withConfig(rest, requireRoot, app.Down)
	case "reload":
		return withConfig(rest, requireRoot, app.Reload)
	case "status":
		return withConfig(rest, noRootRequired, func(cfg *config.Config) error {
			out, err := app.Status(cfg)
			fmt.Print(out)
			return err
		})
	case "validate":
		return withConfig(rest, noRootRequired, func(cfg *config.Config) error {
			fmt.Printf("%s: OK (interface=%s, peers=%d, services=%d)\n",
				configPath(rest), cfg.Interface.Name, len(cfg.Peers), len(cfg.Services))
			return nil
		})
	case "add":
		return add(rest)
	case "export":
		return export(rest)
	case "import":
		return importPeer(rest)
	case "forward":
		return forward(rest)
	case "unforward":
		return unforward(rest)
	case "pubkey":
		return pubkey()
	case "genkey":
		return genkey(rest)
	case "version", "-v", "--version":
		fmt.Println("portman", version)
		return nil
	case "help", "-h", "--help":
		usage()
		return nil
	default:
		usage()
		return fmt.Errorf("unknown command %q", cmd)
	}
}

func usage() {
	fmt.Fprintf(os.Stderr, `portman - WireGuard mesh + DNAT port forwarding manager

Every host runs the same commands. What a host does (accept inbound
connections, forward traffic to a peer) follows from its config, not
from a mode you pick up front.

Fleet (run on the control node, in a checkout of this repo):
  portman add NAME [-endpoint HOST:PORT] [-ansible-host HOST]
                                     put a host into the mesh topology: allocate
                                     the next free tunnel address, mint its
                                     keypair, record it in the Ansible inventory
                                     (private key into the vault), then run
                                     ansible-playbook to converge every host

Setup (run on the host itself, for a mesh you drive by hand instead):
  portman export  [-out FILE] [-address CIDR] [-endpoint HOST:PORT]
                                     generate this host's keypair if needed and
                                     write its descriptor - hand FILE to any host
                                     that should peer with this one
  portman import  DESCRIPTOR_FILE   add the host behind a descriptor as a peer
  portman forward NAME PROTO PORT
  portman forward NAME PROTO [BIND:]PORT:PEER:PEER_PORT
                                     declare a service: with just a PORT, "I run
                                     this here"; with :PEER:PEER_PORT, "forward
                                     PORT to that peer's PEER_PORT"
  portman unforward NAME            remove a forward/service declaration

Apply:
  portman up       [-config PATH]   bring up the tunnel, and DNAT rules for any
                                     forward declarations in this host's config
  portman down     [-config PATH]   tear down the tunnel and any DNAT rules
  portman reload   [-config PATH]   reapply the config to a running tunnel
  portman status   [-config PATH]   show WireGuard and DNAT state
  portman validate [-config PATH]   check the config file without applying it

Low-level:
  portman pubkey                    print this host's WireGuard public key
  portman genkey [-force]           (re)generate this host's WireGuard keypair
  portman version                   print the portman version

portman manages its own WireGuard private key; it is never read from or
written to the config file. Keys live root-only under %s.

Config defaults to %s and must be mode 0600, owned by root.
`, keys.Dir, config.DefaultPath)
}

func configPath(args []string) string {
	fs := flag.NewFlagSet("portman", flag.ContinueOnError)
	fs.SetOutput(os.Stderr)
	path := fs.String("config", config.DefaultPath, "path to config.toml")
	_ = fs.Parse(args)
	return *path
}

const (
	requireRoot    = true
	noRootRequired = false
)

func withConfig(args []string, mustBeRoot bool, fn func(*config.Config) error) error {
	fs := flag.NewFlagSet("portman", flag.ContinueOnError)
	path := fs.String("config", config.DefaultPath, "path to config.toml")
	if err := fs.Parse(args); err != nil {
		return err
	}

	if mustBeRoot && os.Geteuid() != 0 {
		return fmt.Errorf("this command must be run as root")
	}

	cfg, err := config.Load(*path)
	if err != nil {
		return err
	}

	return fn(cfg)
}

// add puts a host into the mesh topology. Addresses are the reason this
// exists: they cannot be picked safely on the host being added, because
// "free" is a fact about every other host. So the topology lives in one
// place - the Ansible inventory - and add is the thing that reads all of
// it, allocates the lowest unused address, and records the new member.
// Nothing is configured on the host here; `ansible-playbook` applies the
// topology afterwards.
func add(args []string) error {
	fs := flag.NewFlagSet("add", flag.ContinueOnError)
	invDir := fs.String("inventory", "", "path to the inventory directory (default: "+inventory.DefaultDir+" in this checkout)")
	endpoint := fs.String("endpoint", "", "host:port peers should dial to reach this host; omit for a host behind NAT")
	address := fs.String("address", "", "tunnel address to pin this host to (default: the lowest free one in portman_network)")
	ansibleHost := fs.String("ansible-host", "", "address ansible should ssh to (default: the host name itself)")
	ansibleUser := fs.String("ansible-user", "root", "user ansible should ssh as")
	passwordFile := fs.String("vault-password-file", "", "vault password file (default: however ansible is already configured)")
	// `portman add NAME -endpoint ...` is how anyone would write this,
	// but flag stops parsing at the first non-flag argument - so lift the
	// name off the front ourselves when it leads, and accept it trailing
	// the flags too.
	name, leading := "", false
	if len(args) > 0 && !strings.HasPrefix(args[0], "-") {
		name, args, leading = args[0], args[1:], true
	}
	if err := fs.Parse(args); err != nil {
		return err
	}
	if !leading {
		name = fs.Arg(0)
	}
	if leftover := fs.NArg(); (leading && leftover > 0) || (!leading && leftover != 1) {
		return fmt.Errorf("usage: portman add NAME [flags]")
	}

	if err := inventory.CheckAvailable(); err != nil {
		return err
	}

	dir := *invDir
	if dir == "" {
		found, err := findInventory()
		if err != nil {
			return err
		}
		dir = found
	}

	inv, err := inventory.Load(dir)
	if err != nil {
		return err
	}

	host, err := inv.Add(inventory.NewHost{
		Name:        name,
		Address:     *address,
		Endpoint:    *endpoint,
		AnsibleHost: *ansibleHost,
		AnsibleUser: *ansibleUser,
	}, inventory.NewVault(inv, *passwordFile))
	if err != nil {
		return err
	}

	prefix, err := inv.Prefix()
	if err != nil {
		return err
	}

	fmt.Printf("added %s to the mesh\n", host.Name)
	fmt.Printf("  address    %s/%d (allocated from %s)\n", host.Address, prefix, inv.Settings.Network)
	fmt.Printf("  public key %s\n", host.PublicKey)
	if host.Endpoint != "" {
		fmt.Printf("  endpoint   %s\n", host.Endpoint)
	} else {
		fmt.Printf("  endpoint   none - dials out only\n")
	}
	fmt.Printf("\nApply the topology to every host (from %s):\n", inv.Root)
	fmt.Printf("  ansible-playbook deploy/playbooks/portman.yml\n")
	return nil
}

// findInventory looks for the inventory in the current directory and its
// parents, so `portman add` works anywhere inside a checkout.
func findInventory() (string, error) {
	dir, err := os.Getwd()
	if err != nil {
		return "", fmt.Errorf("determine working directory: %w", err)
	}
	for {
		candidate := filepath.Join(dir, inventory.DefaultDir)
		if _, err := os.Stat(filepath.Join(candidate, "hosts.yml")); err == nil {
			return candidate, nil
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "", fmt.Errorf("no %s found in this directory or any parent; "+
				"run `portman add` from a portman checkout, or pass -inventory DIR", inventory.DefaultDir)
		}
		dir = parent
	}
}

// export generates this host's persistent keypair if needed and writes a
// small, non-secret descriptor - {name, public_key, address, endpoint} -
// that another host can hand to `portman import` to add this host as a
// peer. If no config exists yet, -address creates a minimal one first.
func export(args []string) error {
	fs := flag.NewFlagSet("export", flag.ContinueOnError)
	path := fs.String("config", config.DefaultPath, "path to config.toml")
	out := fs.String("out", "", "write the descriptor here (default: stdout)")
	name := fs.String("name", "", "name for this host's descriptor (default: hostname)")
	address := fs.String("address", "", "this host's WireGuard address as a CIDR, e.g. 10.10.0.2/24 (only used if no config exists yet)")
	endpoint := fs.String("endpoint", "", "this host's dialable address:port, if peers should be able to connect in")
	iface := fs.String("interface", "wg-portman", "WireGuard interface name (only used if no config exists yet)")
	listenPort := fs.Int("listen-port", 51820, "WireGuard listen port (only used if no config exists yet)")
	if err := fs.Parse(args); err != nil {
		return err
	}

	if os.Geteuid() != 0 {
		return fmt.Errorf("this command must be run as root")
	}

	var cfg *config.Config
	if _, statErr := os.Stat(*path); os.IsNotExist(statErr) {
		if *address == "" {
			return fmt.Errorf("no config at %s yet; pass -address (e.g. -address 10.10.0.2/24) to create one", *path)
		}
		c, err := config.New(*path, *iface, *address, *listenPort)
		if err != nil {
			return err
		}
		cfg = c
	} else if statErr != nil {
		return statErr
	} else {
		c, err := config.Load(*path)
		if err != nil {
			return err
		}
		cfg = c
	}

	pub, err := keys.PublicKey()
	if err != nil {
		return err
	}
	addr, err := cfg.Address()
	if err != nil {
		return err
	}

	hostname := *name
	if hostname == "" {
		hostname, err = os.Hostname()
		if err != nil {
			return fmt.Errorf("determine hostname (pass -name): %w", err)
		}
	}

	desc := config.Descriptor{Name: hostname, PublicKey: pub, Address: addr, Endpoint: *endpoint}

	w := os.Stdout
	if *out != "" {
		f, err := os.OpenFile(*out, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, 0o644)
		if err != nil {
			return fmt.Errorf("write %s: %w", *out, err)
		}
		defer f.Close()
		w = f
	}
	if err := toml.NewEncoder(w).Encode(desc); err != nil {
		return fmt.Errorf("encode descriptor: %w", err)
	}
	if *out != "" {
		fmt.Fprintf(os.Stderr, "wrote %s - hand it to any host that should peer with %q\n", *out, hostname)
	}
	return nil
}

// importPeer reads a descriptor written by `portman export` on another
// host and adds (or updates) it as a [[peer]] in this host's config.
func importPeer(args []string) error {
	fs := flag.NewFlagSet("import", flag.ContinueOnError)
	path := fs.String("config", config.DefaultPath, "path to config.toml")
	allowedIPs := fs.String("allowed-ips", "", "override allowed_ips (comma-separated CIDRs); default is the descriptor's address/32")
	keepalive := fs.Int("persistent-keepalive", 25, "persistent_keepalive seconds (0 to disable)")
	psk := fs.String("preshared-key", "", "optional preshared key for this peer")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if fs.NArg() != 1 {
		return fmt.Errorf("usage: portman import DESCRIPTOR_FILE")
	}
	descPath := fs.Arg(0)

	if os.Geteuid() != 0 {
		return fmt.Errorf("this command must be run as root")
	}

	var desc config.Descriptor
	if _, err := toml.DecodeFile(descPath, &desc); err != nil {
		return fmt.Errorf("parse %s: %w", descPath, err)
	}
	if desc.Name == "" || desc.PublicKey == "" || desc.Address == "" {
		return fmt.Errorf("%s is not a valid portman descriptor (missing name/public_key/address)", descPath)
	}

	cfg, err := config.Load(*path)
	if err != nil {
		return err
	}

	ips := []string{desc.Address + "/32"}
	if *allowedIPs != "" {
		ips = strings.Split(*allowedIPs, ",")
	}

	cfg.AddOrReplacePeer(config.Peer{
		Name:                desc.Name,
		PublicKey:           desc.PublicKey,
		PresharedKey:        *psk,
		Endpoint:            desc.Endpoint,
		AllowedIPs:          ips,
		PersistentKeepalive: *keepalive,
	})
	if err := cfg.Save(*path); err != nil {
		return err
	}

	fmt.Printf("imported peer %q (%s) into %s\n", desc.Name, desc.Address, *path)
	fmt.Println("run `sudo portman up` (or `reload` if already running) to apply")
	return nil
}

// forward adds or replaces a [[service]] declaration: a bare port is a
// local declaration ("I run this here"); PORT:PEER:PEER_PORT (optionally
// prefixed with BIND:) is a forward rule to an already-imported peer.
func forward(args []string) error {
	fs := flag.NewFlagSet("forward", flag.ContinueOnError)
	path := fs.String("config", config.DefaultPath, "path to config.toml")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if fs.NArg() != 3 {
		return fmt.Errorf("usage: portman forward NAME PROTO PORT | portman forward NAME PROTO [BIND:]PORT:PEER:PEER_PORT")
	}
	name, proto, spec := fs.Arg(0), fs.Arg(1), fs.Arg(2)

	if os.Geteuid() != 0 {
		return fmt.Errorf("this command must be run as root")
	}

	cfg, err := config.Load(*path)
	if err != nil {
		return err
	}

	svc, err := buildService(cfg, name, proto, spec)
	if err != nil {
		return err
	}

	cfg.AddOrReplaceService(svc)
	if err := cfg.Save(*path); err != nil {
		return err
	}

	if svc.IsForward() {
		fmt.Printf("saved service %q: forward %s -> %s\n", name, svc.RemoteBind, net.JoinHostPort(svc.ClientAddress, strconv.Itoa(svc.LocalPort)))
	} else {
		fmt.Printf("saved service %q: local port %d\n", name, svc.LocalPort)
	}
	fmt.Println("run `sudo portman up` (or `reload` if already running) to apply")
	return nil
}

func buildService(cfg *config.Config, name, proto, spec string) (config.Service, error) {
	parts := strings.Split(spec, ":")

	switch len(parts) {
	case 1:
		port, err := strconv.Atoi(parts[0])
		if err != nil || port <= 0 {
			return config.Service{}, fmt.Errorf("invalid port %q", parts[0])
		}
		return config.Service{Name: name, Protocol: proto, LocalPort: port}, nil

	case 3, 4:
		bindHost, bindPort, peerName, peerPortStr := "0.0.0.0", parts[0], parts[1], parts[2]
		if len(parts) == 4 {
			bindHost, bindPort, peerName, peerPortStr = parts[0], parts[1], parts[2], parts[3]
		}
		if _, err := strconv.Atoi(bindPort); err != nil {
			return config.Service{}, fmt.Errorf("invalid port %q", bindPort)
		}
		peerPort, err := strconv.Atoi(peerPortStr)
		if err != nil || peerPort <= 0 {
			return config.Service{}, fmt.Errorf("invalid peer port %q", peerPortStr)
		}

		peer, ok := cfg.PeerByName(peerName)
		if !ok {
			return config.Service{}, fmt.Errorf("no peer named %q; run `portman import` first", peerName)
		}
		addr, err := peerAddress(peer)
		if err != nil {
			return config.Service{}, fmt.Errorf("peer %q: %w", peerName, err)
		}

		return config.Service{
			Name:          name,
			Protocol:      proto,
			RemoteBind:    net.JoinHostPort(bindHost, bindPort),
			ClientAddress: addr,
			LocalPort:     peerPort,
		}, nil

	default:
		return config.Service{}, fmt.Errorf("invalid forward spec %q: want PORT, or PORT:PEER:PEER_PORT, or BIND:PORT:PEER:PEER_PORT", spec)
	}
}

// peerAddress extracts a single host address from a peer's allowed_ips,
// as set by `portman import` (descriptor address + "/32").
func peerAddress(p config.Peer) (string, error) {
	if len(p.AllowedIPs) != 1 {
		return "", fmt.Errorf("has %d allowed_ips, not a single address; set client_address by editing the config directly", len(p.AllowedIPs))
	}
	if ip, _, err := net.ParseCIDR(p.AllowedIPs[0]); err == nil {
		return ip.String(), nil
	}
	if ip := net.ParseIP(p.AllowedIPs[0]); ip != nil {
		return ip.String(), nil
	}
	return "", fmt.Errorf("allowed_ips %q is not a single host address", p.AllowedIPs[0])
}

// unforward removes a service declaration by name.
func unforward(args []string) error {
	fs := flag.NewFlagSet("unforward", flag.ContinueOnError)
	path := fs.String("config", config.DefaultPath, "path to config.toml")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if fs.NArg() != 1 {
		return fmt.Errorf("usage: portman unforward NAME")
	}
	name := fs.Arg(0)

	if os.Geteuid() != 0 {
		return fmt.Errorf("this command must be run as root")
	}

	cfg, err := config.Load(*path)
	if err != nil {
		return err
	}
	if !cfg.RemoveService(name) {
		return fmt.Errorf("no service named %q", name)
	}
	if err := cfg.Save(*path); err != nil {
		return err
	}

	fmt.Printf("removed service %q\n", name)
	fmt.Println("run `sudo portman up` (or `reload` if already running) to apply")
	return nil
}

// pubkey prints this host's persistent WireGuard public key, generating
// a keypair on first use if none exists yet. The private key never
// leaves the host; only this value needs to be shared with a peer.
func pubkey() error {
	if os.Geteuid() != 0 {
		return fmt.Errorf("this command must be run as root")
	}
	pub, err := keys.PublicKey()
	if err != nil {
		return err
	}
	fmt.Println(pub)
	return nil
}

// genkey (re)generates this host's WireGuard keypair. Regenerating an
// existing keypair breaks the tunnel until every peer's config is updated
// with the new public key, so it requires -force.
func genkey(args []string) error {
	if os.Geteuid() != 0 {
		return fmt.Errorf("this command must be run as root")
	}

	fs := flag.NewFlagSet("genkey", flag.ContinueOnError)
	force := fs.Bool("force", false, "overwrite an existing keypair")
	if err := fs.Parse(args); err != nil {
		return err
	}

	if _, err := os.Stat(keys.PrivateKeyPath); err == nil && !*force {
		return fmt.Errorf("a keypair already exists at %s; pass -force to replace it "+
			"(you'll need to update the public key on every peer)", keys.PrivateKeyPath)
	}

	_, pub, err := keys.Generate()
	if err != nil {
		return err
	}

	fmt.Println(pub)
	return nil
}

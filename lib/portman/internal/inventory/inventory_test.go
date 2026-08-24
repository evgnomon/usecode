package inventory

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// newInventory writes a minimal inventory to a temp dir and loads it.
// hostVars maps a host name to the body of its host_vars file; every one
// of them is also put in the portman group.
func newInventory(t *testing.T, settings string, hostVars map[string]string) *Inventory {
	t.Helper()

	dir := filepath.Join(t.TempDir(), "inventory")
	mustWrite(t, filepath.Join(dir, "group_vars", Group, "main.yml"), settings)

	hosts := "all:\n  children:\n    portman:\n      hosts:\n"
	if len(hostVars) == 0 {
		hosts = "all:\n  children:\n    portman:\n      hosts: {}\n"
	}
	for name, body := range hostVars {
		hosts += "        " + name + ":\n"
		mustWrite(t, filepath.Join(dir, "host_vars", name+".yml"), body)
	}
	mustWrite(t, filepath.Join(dir, "hosts.yml"), hosts)

	inv, err := Load(dir)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	return inv
}

func mustWrite(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

const defaultSettings = "portman_network: 10.10.0.0/24\nportman_address_start: 1\nportman_listen_port: 51820\n"

func hostVars(address string) string {
	return "portman_address: " + address + "\nportman_public_key: xxx=\n"
}

// Allocation is the whole reason the topology is central: the next
// address is a fact about every host, so it has to be read off all of
// them at once.
func TestAllocateSkipsTakenAddresses(t *testing.T) {
	inv := newInventory(t, defaultSettings, map[string]string{
		"edge":   hostVars("10.10.0.1"),
		"laptop": hostVars("10.10.0.2"),
		"phone":  hostVars("10.10.0.4"),
	})

	got, err := inv.allocate()
	if err != nil {
		t.Fatalf("allocate: %v", err)
	}
	if want := "10.10.0.3"; got != want {
		t.Errorf("allocate() = %s, want %s (the gap between .2 and .4)", got, want)
	}
}

func TestAllocateOnEmptyMeshStartsAtAddressStart(t *testing.T) {
	inv := newInventory(t, "portman_network: 10.10.0.0/24\nportman_address_start: 10\n", nil)

	got, err := inv.allocate()
	if err != nil {
		t.Fatalf("allocate: %v", err)
	}
	if want := "10.10.0.10"; got != want {
		t.Errorf("allocate() = %s, want %s", got, want)
	}
}

// A full network must be reported, not silently wrapped around to an
// address someone already holds.
func TestAllocateReportsAFullNetwork(t *testing.T) {
	// A /30 holds exactly two usable addresses: .1 and .2.
	inv := newInventory(t, "portman_network: 10.10.0.0/30\nportman_address_start: 1\n", map[string]string{
		"a": hostVars("10.10.0.1"),
		"b": hostVars("10.10.0.2"),
	})

	if _, err := inv.allocate(); err == nil {
		t.Fatal("allocate() on a full network returned no error")
	} else if !strings.Contains(err.Error(), "no free address") {
		t.Errorf("allocate() error = %q, want it to say the network is full", err)
	}
}

// The clash this package exists to prevent: if the topology already
// contains one, adding a host must stop rather than build on top of it.
func TestAllocateRefusesAnAlreadyClashingTopology(t *testing.T) {
	inv := newInventory(t, defaultSettings, map[string]string{
		"laptop": hostVars("10.10.0.2"),
		"phone":  hostVars("10.10.0.2"),
	})

	_, err := inv.allocate()
	if err == nil {
		t.Fatal("allocate() on a clashing topology returned no error")
	}
	if !strings.Contains(err.Error(), "both claim 10.10.0.2") {
		t.Errorf("allocate() error = %q, want it to name the clashing address", err)
	}
}

func TestCheckAddress(t *testing.T) {
	inv := newInventory(t, defaultSettings, map[string]string{"laptop": hostVars("10.10.0.2")})

	for _, tc := range []struct {
		address string
		want    string // substring of the expected error, "" for accepted
	}{
		{"10.10.0.7", ""},
		{"10.10.0.2", "already held by laptop"},
		{"192.168.1.5", "outside portman_network"},
		{"10.10.0.5/24", "not an IP address"},
	} {
		err := inv.checkAddress(tc.address)
		switch {
		case tc.want == "" && err != nil:
			t.Errorf("checkAddress(%s) = %v, want accepted", tc.address, err)
		case tc.want != "" && err == nil:
			t.Errorf("checkAddress(%s) accepted, want %q", tc.address, tc.want)
		case tc.want != "" && err != nil && !strings.Contains(err.Error(), tc.want):
			t.Errorf("checkAddress(%s) = %q, want it to mention %q", tc.address, err, tc.want)
		}
	}
}

// Adding a host has to leave hosts.yml usable by Ansible and by the next
// `portman add`, comments and all.
func TestAddToGroupPreservesTheFile(t *testing.T) {
	inv := newInventory(t, defaultSettings, map[string]string{"edge": hostVars("10.10.0.1")})

	original, err := os.ReadFile(inv.hostsPath())
	if err != nil {
		t.Fatal(err)
	}
	mustWrite(t, inv.hostsPath(), "---\n# keep me\n"+string(original))

	if err := inv.addToGroup("laptop"); err != nil {
		t.Fatalf("addToGroup: %v", err)
	}

	got, err := os.ReadFile(inv.hostsPath())
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(got), "# keep me") {
		t.Errorf("addToGroup dropped the file's comments:\n%s", got)
	}
	if !strings.HasPrefix(string(got), "---\n") {
		t.Errorf("addToGroup dropped the document start:\n%s", got)
	}

	mustWrite(t, inv.hostVarsPath("laptop"), hostVars("10.10.0.2"))
	reloaded, err := Load(inv.Dir)
	if err != nil {
		t.Fatalf("reload after addToGroup: %v", err)
	}
	if _, ok := reloaded.Host("laptop"); !ok {
		t.Errorf("laptop is not in the group after addToGroup; file is:\n%s", got)
	}
	if _, ok := reloaded.Host("edge"); !ok {
		t.Errorf("addToGroup lost the host that was already there; file is:\n%s", got)
	}
}

// An empty group is written `hosts: {}`, and a group with no hosts key at
// all is legal Ansible too - both have to accept the first member.
func TestAddToGroupFromAnEmptyGroup(t *testing.T) {
	for name, hostsYAML := range map[string]string{
		"flow mapping": "all:\n  children:\n    portman:\n      hosts: {}\n",
		"null hosts":   "all:\n  children:\n    portman:\n      hosts:\n",
		"no hosts key": "all:\n  children:\n    portman:\n",
		"no children":  "all:\n  hosts: {}\n",
	} {
		t.Run(name, func(t *testing.T) {
			inv := newInventory(t, defaultSettings, nil)
			mustWrite(t, inv.hostsPath(), hostsYAML)

			if err := inv.addToGroup("edge"); err != nil {
				t.Fatalf("addToGroup: %v", err)
			}
			mustWrite(t, inv.hostVarsPath("edge"), hostVars("10.10.0.1"))

			reloaded, err := Load(inv.Dir)
			if err != nil {
				t.Fatalf("reload: %v", err)
			}
			if _, ok := reloaded.Host("edge"); !ok {
				body, _ := os.ReadFile(inv.hostsPath())
				t.Errorf("edge is not in the group; file is:\n%s", body)
			}
		})
	}
}

func TestEndpointGetsTheMeshPort(t *testing.T) {
	inv := newInventory(t, defaultSettings, nil)

	for in, want := range map[string]string{
		"":                      "",
		"vpn.example.com":       "vpn.example.com:51820",
		"vpn.example.com:51821": "vpn.example.com:51821",
		"203.0.113.10":          "203.0.113.10:51820",
		"[2001:db8::1]:51820":   "[2001:db8::1]:51820",
	} {
		if got := inv.endpoint(in); got != want {
			t.Errorf("endpoint(%q) = %q, want %q", in, got, want)
		}
	}
}

// A host that is already a member must not be re-added: minting a second
// keypair would break the tunnel it already has.
func TestAddRefusesAnExistingHost(t *testing.T) {
	inv := newInventory(t, defaultSettings, map[string]string{"edge": hostVars("10.10.0.1")})

	_, err := inv.Add(NewHost{Name: "edge"}, &Vault{Path: filepath.Join(t.TempDir(), "secrets.yml")})
	if err == nil {
		t.Fatal("Add() of an existing host returned no error")
	}
	if !strings.Contains(err.Error(), "already in the mesh") {
		t.Errorf("Add() error = %q, want it to say the host is already in the mesh", err)
	}
}

// A leftover host_vars file with no group membership is a half-added
// host; adding on top of it would orphan whatever key it names.
func TestAddRefusesAnOrphanedHostVarsFile(t *testing.T) {
	inv := newInventory(t, defaultSettings, nil)
	mustWrite(t, inv.hostVarsPath("edge"), hostVars("10.10.0.1"))

	_, err := inv.Add(NewHost{Name: "edge"}, &Vault{Path: filepath.Join(t.TempDir(), "secrets.yml")})
	if err == nil {
		t.Fatal("Add() over an orphaned host_vars file returned no error")
	}
	if !strings.Contains(err.Error(), "not in the portman group") {
		t.Errorf("Add() error = %q, want it to point at the group membership", err)
	}
}

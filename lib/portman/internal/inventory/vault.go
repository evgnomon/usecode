package inventory

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"

	"gopkg.in/yaml.v3"

	"github.com/evgnomon/portman/internal/keys"
)

// PrivateKeysVar is the single mapping, host name -> WireGuard private
// key, that the vault file holds. Keeping every host in one file (rather
// than one vaulted file per host) is what lets a playbook resolve any
// host's key by name: portman_private_keys[inventory_hostname].
const PrivateKeysVar = "portman_private_keys"

// Vault is the ansible-vault encrypted secrets file. portman shells out
// to ansible-vault rather than implementing the format, so the file is
// exactly what `ansible-vault edit` and a playbook expect, and the
// password comes from wherever ansible normally finds it (a prompt,
// --vault-password-file, ANSIBLE_VAULT_PASSWORD_FILE, or ansible.cfg in
// the directory the playbooks run from).
type Vault struct {
	// Path is the secrets file, encrypted in place.
	Path string
	// Dir is the working directory ansible-vault runs in, so that an
	// ansible.cfg there (with e.g. vault_password_file) applies.
	Dir string
	// PasswordFile, when set, is passed as --vault-password-file.
	PasswordFile string
}

// NewVault returns the vault for inv's secrets file.
func NewVault(inv *Inventory, passwordFile string) *Vault {
	return &Vault{Path: inv.SecretsPath(), Dir: inv.Root, PasswordFile: passwordFile}
}

// Put stores one host's private key, leaving every other host's alone.
// If the file doesn't exist yet it is created and encrypted.
func (v *Vault) Put(host, privateKey string) error {
	secrets, err := v.load()
	if err != nil {
		return err
	}
	if _, exists := secrets[host]; exists {
		return fmt.Errorf("%s already has a private key in %s", host, v.Path)
	}
	secrets[host] = privateKey
	return v.save(secrets)
}

// load decrypts the secrets file and returns the host -> private key
// mapping, or an empty mapping if the file doesn't exist yet.
func (v *Vault) load() (map[string]string, error) {
	raw, err := os.ReadFile(v.Path)
	if err != nil {
		if os.IsNotExist(err) {
			return map[string]string{}, nil
		}
		return nil, fmt.Errorf("read %s: %w", v.Path, err)
	}

	// A file that isn't encrypted yet is still readable - it gets
	// encrypted on the way back out, so a hand-created plaintext
	// secrets file converges to a vaulted one rather than erroring.
	plaintext := raw
	if bytes.HasPrefix(bytes.TrimSpace(raw), []byte("$ANSIBLE_VAULT")) {
		plaintext, err = v.decrypt()
		if err != nil {
			return nil, err
		}
	}

	var doc map[string]map[string]string
	if err := yaml.Unmarshal(plaintext, &doc); err != nil {
		return nil, fmt.Errorf("parse %s: %w", v.Path, err)
	}
	if doc[PrivateKeysVar] == nil {
		return map[string]string{}, nil
	}
	return doc[PrivateKeysVar], nil
}

func (v *Vault) decrypt() ([]byte, error) {
	cmd := v.command("decrypt", "--output=-", v.Path)
	var out bytes.Buffer
	cmd.Stdout = &out
	if err := cmd.Run(); err != nil {
		return nil, fmt.Errorf("decrypt %s: %w", v.Path, err)
	}
	return out.Bytes(), nil
}

// save writes secrets back, encrypted. The plaintext is staged in a
// root-of-the-vault sibling directory created mode 0700 rather than
// piped through stdin, because ansible-vault needs stdin free to prompt
// for the vault password.
func (v *Vault) save(secrets map[string]string) error {
	var body bytes.Buffer
	enc := yaml.NewEncoder(&body)
	enc.SetIndent(2)
	if err := enc.Encode(map[string]map[string]string{PrivateKeysVar: secrets}); err != nil {
		return fmt.Errorf("encode secrets: %w", err)
	}
	if err := enc.Close(); err != nil {
		return fmt.Errorf("encode secrets: %w", err)
	}

	names := make([]string, 0, len(secrets))
	for name := range secrets {
		names = append(names, name)
	}
	sort.Strings(names)

	header := fmt.Sprintf(`---
# ansible-vault encrypted: every mesh host's WireGuard private key, keyed
# by inventory hostname (%s), so a play can resolve the key for
# the host it is running against. Currently: %s.
#
# Edit with: ansible-vault edit %s
`, PrivateKeysVar, strings.Join(names, ", "), filepath.Base(v.Path))
	plaintext := append([]byte(header), body.Bytes()...)

	if err := os.MkdirAll(filepath.Dir(v.Path), 0o755); err != nil {
		return fmt.Errorf("create %s: %w", filepath.Dir(v.Path), err)
	}
	staging, err := os.MkdirTemp(filepath.Dir(v.Path), ".portman-vault-")
	if err != nil {
		return fmt.Errorf("create staging directory: %w", err)
	}
	defer os.RemoveAll(staging)

	tmp := filepath.Join(staging, "secrets.yml")
	if err := os.WriteFile(tmp, plaintext, 0o600); err != nil {
		return fmt.Errorf("write %s: %w", tmp, err)
	}

	cmd := v.command("encrypt", "--output="+v.Path, tmp)
	cmd.Stdout = os.Stderr // progress messages, not data
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("encrypt %s: %w", v.Path, err)
	}
	return nil
}

// command builds an ansible-vault invocation with stdin and stderr
// attached so a password prompt reaches the terminal.
func (v *Vault) command(args ...string) *exec.Cmd {
	if v.PasswordFile != "" {
		args = append(args, "--vault-password-file", v.PasswordFile)
	}
	cmd := exec.Command("ansible-vault", args...)
	cmd.Dir = v.Dir
	cmd.Stdin = os.Stdin
	cmd.Stderr = os.Stderr
	return cmd
}

// CheckAvailable reports a friendly error if ansible-vault isn't
// installed, before anything has been written.
func CheckAvailable() error {
	if _, err := exec.LookPath("ansible-vault"); err != nil {
		return fmt.Errorf("ansible-vault not found in PATH; install ansible on this machine " +
			"(the private key for a new host has to be stored in the vault)")
	}
	return nil
}

// newKeypair mints the WireGuard keypair for a host being added. It runs
// on the control node, so it deliberately does not touch this machine's
// own key directory.
func newKeypair() (privateKey, publicKey string, err error) {
	return keys.NewPair()
}

// marshalDoc encodes a parsed YAML document back to bytes at the
// two-space indent Ansible files conventionally use.
func marshalDoc(doc *yaml.Node) ([]byte, error) {
	var buf bytes.Buffer
	enc := yaml.NewEncoder(&buf)
	enc.SetIndent(2)
	if err := enc.Encode(doc); err != nil {
		return nil, err
	}
	if err := enc.Close(); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

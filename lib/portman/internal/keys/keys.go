// Package keys manages this host's persistent WireGuard keypair so users
// never have to generate or paste a private key themselves. The private
// key is generated on first use and stored root-only on disk; only the
// public key (safe to share) is ever meant to leave the host.
package keys

import (
	"bytes"
	"crypto/ecdh"
	"crypto/rand"
	"encoding/base64"
	"fmt"
	"os"
	"os/exec"
	"strings"
)

// Dir is where this host's WireGuard keypair is stored. It must stay
// root-only: anyone who reads private.key can impersonate this host on
// the tunnel.
const Dir = "/etc/portman/wireguard"

// PrivateKeyPath and PublicKeyPath are the files under Dir.
const (
	PrivateKeyPath = Dir + "/private.key"
	PublicKeyPath  = Dir + "/public.key"
)

// EnsurePrivateKey returns this host's persistent WireGuard private key,
// generating and storing a new keypair on first call if none exists yet.
func EnsurePrivateKey() (string, error) {
	if data, err := os.ReadFile(PrivateKeyPath); err == nil {
		return strings.TrimSpace(string(data)), nil
	} else if !os.IsNotExist(err) {
		return "", fmt.Errorf("read %s: %w", PrivateKeyPath, err)
	}

	priv, _, err := Generate()
	return priv, err
}

// PublicKey returns this host's persistent WireGuard public key,
// generating a keypair first if one doesn't exist yet.
func PublicKey() (string, error) {
	if data, err := os.ReadFile(PublicKeyPath); err == nil {
		return strings.TrimSpace(string(data)), nil
	} else if !os.IsNotExist(err) {
		return "", fmt.Errorf("read %s: %w", PublicKeyPath, err)
	}

	_, pub, err := Generate()
	return pub, err
}

// Generate creates a new WireGuard keypair, persists it root-only under
// Dir, and returns (privateKey, publicKey). It overwrites any existing
// keypair, so a peer's config must be updated with the new public key
// afterwards.
func Generate() (privateKey, publicKey string, err error) {
	priv, pub, err := NewPair()
	if err != nil {
		return "", "", err
	}

	if err := persist(priv, pub); err != nil {
		return "", "", err
	}

	return priv, pub, nil
}

// NewPair creates a WireGuard keypair without touching the filesystem.
// It is for keys that belong to some *other* host - `portman add` mints
// a new mesh member's keypair on the control node, where Dir is neither
// writable nor the right place for it.
func NewPair() (privateKey, publicKey string, err error) {
	priv, err := genPrivateKey()
	if err != nil {
		return "", "", err
	}

	pub, err := derivePublicKey(priv)
	if err != nil {
		return "", "", err
	}

	return priv, pub, nil
}

func persist(priv, pub string) error {
	if err := os.MkdirAll(Dir, 0o700); err != nil {
		return fmt.Errorf("create %s: %w", Dir, err)
	}
	// MkdirAll doesn't change the mode of a pre-existing directory.
	if err := os.Chmod(Dir, 0o700); err != nil {
		return fmt.Errorf("chmod %s: %w", Dir, err)
	}

	tmp := PrivateKeyPath + ".tmp"
	if err := os.WriteFile(tmp, []byte(priv+"\n"), 0o600); err != nil {
		return fmt.Errorf("write %s: %w", PrivateKeyPath, err)
	}
	if err := os.Rename(tmp, PrivateKeyPath); err != nil {
		return fmt.Errorf("write %s: %w", PrivateKeyPath, err)
	}

	// The public key is not secret, but keep it out of group/other reach
	// too so the key directory has one consistent, easily-audited mode.
	if err := os.WriteFile(PublicKeyPath, []byte(pub+"\n"), 0o600); err != nil {
		return fmt.Errorf("write %s: %w", PublicKeyPath, err)
	}

	return nil
}

// genPrivateKey uses wg(8) when available, so key generation matches
// exactly what the rest of the WireGuard ecosystem produces, falling
// back to generating the Curve25519 scalar directly.
func genPrivateKey() (string, error) {
	out, err := exec.Command("wg", "genkey").Output()
	if err == nil {
		return strings.TrimSpace(string(out)), nil
	}

	var raw [32]byte
	if _, rerr := rand.Read(raw[:]); rerr != nil {
		return "", fmt.Errorf("generate key: %w", rerr)
	}
	raw[0] &= 248
	raw[31] = (raw[31] & 127) | 64
	return base64.StdEncoding.EncodeToString(raw[:]), nil
}

// derivePublicKey turns a base64 Curve25519 private key into its public
// half. It prefers wg(8) so the result is byte-for-byte what the rest of
// the WireGuard ecosystem produces, and falls back to the standard
// library's X25519 (the same RFC 7748 clamped scalar multiplication) so
// that a control node without wireguard-tools can still mint a keypair
// for a host it is adding to the mesh.
func derivePublicKey(priv string) (string, error) {
	cmd := exec.Command("wg", "pubkey")
	cmd.Stdin = bytes.NewReader([]byte(priv + "\n"))
	out, err := cmd.Output()
	if err == nil {
		return strings.TrimSpace(string(out)), nil
	}

	raw, decErr := base64.StdEncoding.DecodeString(strings.TrimSpace(priv))
	if decErr != nil {
		return "", fmt.Errorf("private key is not valid base64: %w", decErr)
	}
	key, keyErr := ecdh.X25519().NewPrivateKey(raw)
	if keyErr != nil {
		return "", fmt.Errorf("derive public key: %w", keyErr)
	}
	return base64.StdEncoding.EncodeToString(key.PublicKey().Bytes()), nil
}

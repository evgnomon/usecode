/*
Copyright (C) 2025- Hamed Ghasemzadeh. All rights reserved.
License: HGL General License <https://evgnomon.org/docs/hgl>
*/
package utils

import (
	"bytes"
	"crypto/rand"
	_ "embed"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"strings"

	toml "github.com/pelletier/go-toml/v2"
)

var logger = NewLogger()

//go:embed scripts/vault_pass
var vaultPassScript string

var pattern = regexp.MustCompile(`^[a-zA-Z_][a-zA-Z0-9_$]{0,63}$`)

type VaultConfig struct {
	DefaultKey string `toml:"default_key"`
}

type ZygoteConfig struct {
	Vault VaultConfig
}

const (
	executablePerm = 0755
	letters        = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
	randLen        = 16
)

func randomString(n int) (string, error) {
	b := make([]byte, n)
	_, err := rand.Read(b)
	if err != nil {
		return "", err
	}
	for i, bv := range b {
		b[i] = letters[int(bv)%len(letters)]
	}
	return string(b), nil
}

func ReadConfig() (*ZygoteConfig, error) {
	doc, err := os.ReadFile(filepath.Join(os.Getenv("HOME"), ".config", "zygote", "config.toml"))
	if err != nil {
		return nil, err
	}
	config := ZygoteConfig{}
	err = toml.Unmarshal(doc, &config)
	if err != nil {
		return nil, err
	}
	return &config, nil
}

var (
	secretsDir = filepath.Join(os.Getenv("HOME"), ".config", "usecode", "secrets")
)

type RunOpts struct {
	SupressStderr bool
	SupressStdout bool
}

func RunWithOpts(opts RunOpts, argv ...string) error {
	qoutedArgv := make([]string, len(argv))
	for i, arg := range argv {
		qoutedArgv[i] = fmt.Sprintf("%q", arg)
	}
	cmd := exec.Command("/bin/sh", "-c", strings.Join(qoutedArgv, " ")) // #nosec
	if !opts.SupressStderr {
		cmd.Stderr = os.Stderr
	}
	if !opts.SupressStdout {
		cmd.Stdout = os.Stdout
	}
	cmd.Stdin = os.Stdin
	cmd.Env = os.Environ()
	err := cmd.Run()
	if err != nil {
		return fmt.Errorf("run %v failed: %v", argv, err)
	}
	return nil
}

func Run(argv ...string) error {
	return RunWithOpts(RunOpts{}, argv...)
}

func RunSilent(argv ...string) error {
	return RunWithOpts(RunOpts{
		SupressStderr: true,
		SupressStdout: true,
	}, argv...)
}

func RunCapture(argv ...string) (string, error) {
	qoutedArgv := make([]string, len(argv))
	for i, arg := range argv {
		qoutedArgv[i] = fmt.Sprintf("%q", arg)
	}
	cmd := exec.Command("/bin/sh", "-c", strings.Join(qoutedArgv, " ")) // #nosec
	cmd.Stdout = os.Stdout
	output, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("run %v failed: %v", argv, err)
	}
	return string(output), nil
}

func Script(commands [][]string) error {
	for _, cmd := range commands {
		err := Run(cmd...)
		if err != nil {
			return fmt.Errorf("failed to running command: %w", err)
		}
	}
	return nil
}

func PathExists(path string) bool {
	_, err := os.Stat(path)
	if err != nil {
		if os.IsNotExist(err) {
			return false
		}
		logger.FatalIfErr("Checking path", err, M{"path": path})
	}
	return true
}

func DirExists(path string) (bool, error) {
	stat, err := os.Stat(path)
	if err != nil {
		if os.IsNotExist(err) {
			return false, nil
		}
		return false, fmt.Errorf("error getting path %s: %v", path, err)
	}
	return stat.IsDir(), nil
}

func Chdir(path string) error {
	return os.Chdir(path)
}

func Elevate() error {
	err := Run("sudo", "-v")
	return err
}

func UnElevate() error {
	err := Run("sudo", "-K")
	return err
}

func User() string {
	return os.Getenv("USER")
}

func UserHome() string {
	return os.Getenv("HOME")
}

func EncryptFile(filename, gpgKey string) error {
	if filename == "" {
		return fmt.Errorf("please provide a file to encrypt")
	}

	zc, err := ReadConfig()
	if err != nil {
		return fmt.Errorf("failed to read config: %v", err)
	}

	// Use default key if none provided
	if gpgKey == "" {
		gpgKey = zc.Vault.DefaultKey
	}

	// Create secrets directory if it doesn't exist
	err = os.MkdirAll(secretsDir, executablePerm)
	if err != nil {
		return fmt.Errorf("failed to create secrets directory: %v", err)
	}

	// Construct output filename
	outputFile := filepath.Join(secretsDir, filename+".asc")

	// Run gpg command
	cmd := exec.Command("gpg", "-e", "-r", gpgKey, "--armor", "-o", outputFile)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	err = cmd.Run()
	if err != nil {
		return fmt.Errorf("encryption failed: %v", err)
	}

	return nil
}

func EncryptContent(content, filename, gpgKey string) (string, error) {
	if filename == "" {
		return "", fmt.Errorf("please provide a file to encrypt")
	}

	zc, err := ReadConfig()
	if err != nil {
		return "", fmt.Errorf("failed to read config: %v", err)
	}

	// Use default key if none provided
	if gpgKey == "" {
		gpgKey = zc.Vault.DefaultKey
	}

	// Create secrets directory if it doesn't exist
	err = os.MkdirAll(secretsDir, executablePerm)
	if err != nil {
		return "", fmt.Errorf("failed to create secrets directory: %v", err)
	}

	// Construct output filename
	outputFile := filepath.Join(secretsDir, filename+".asc")

	// Run gpg command
	cmd := exec.Command("gpg", "-e", "-r", gpgKey, "--armor", "-o", outputFile)
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return "", fmt.Errorf("failed to create stdin: %v", err)
	}
	var out bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = os.Stderr
	if err := cmd.Start(); err != nil {
		return "", fmt.Errorf("failed to start: %v", err)
	}

	_, err = io.WriteString(stdin, content)
	if err != nil {
		return "", fmt.Errorf("failed to write to stdin: %v", err)
	}
	stdin.Close()

	if err := cmd.Wait(); err != nil {
		return "", fmt.Errorf("failed to wait: %v", err)
	}

	return out.String(), nil
}

func DecryptFile(filename string) error {
	if filename == "" {
		return fmt.Errorf("please provide a file to decrypt")
	}

	// Construct full path to encrypted file
	inputFile := filepath.Join(secretsDir, filename+".asc")

	// Run gpg command
	cmd := exec.Command("gpg", "--quiet", "-d", inputFile)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	err := cmd.Run()
	if err != nil {
		return fmt.Errorf("decryption failed: %v", err)
	}

	return nil
}

func RepoFullName() string {
	repoPath, err := os.Getwd()
	logger.FatalIfErr("Error getting current directory", err)
	org := filepath.Base(filepath.Dir(repoPath))
	repo := filepath.Base(repoPath)
	formatted := fmt.Sprintf("%s_%s", org, repo)
	if !pattern.MatchString(formatted) {
		logger.FatalIfErr("Error creating repo name", err)
	}
	return formatted
}

func RepoName() string {
	repoPath, err := os.Getwd()
	logger.FatalIfErr("Error getting current directory", err)
	repo := filepath.Base(repoPath)
	return repo
}

func RepoVaultPath() string {
	vaultAddress := RepoFullName()
	secretFile := filepath.Join(UserHome(), ".config", "usecode", "secrets", fmt.Sprintf("%s.yaml", vaultAddress))
	return secretFile
}

func CreateRepoVault() error {
	vaultAddress := RepoFullName()
	vaultFile := fmt.Sprintf("%s.vault", vaultAddress)
	vaultFileExist := PathExists(filepath.Join(UserHome(), ".config", "usecode", "secrets", fmt.Sprintf("%s.asc", vaultFile)))
	s, err := randomString(randLen)
	logger.FatalIfErr("Error generating random string", err)
	if vaultFileExist {
		return nil
	}
	_, err = EncryptContent(s, fmt.Sprintf("%s.yaml", vaultAddress), "")
	if err != nil {
		return err
	}
	return nil
}

func WriteScripts() error {
	err := os.MkdirAll(filepath.Join(UserHome(), ".config", "zygote", "scripts"), executablePerm)
	if err != nil {
		return fmt.Errorf("failed to create scripts directory: %v", err)
	}
	err = os.WriteFile(
		filepath.Join(UserHome(), ".config", "zygote", "scripts", "vault_pass"),
		[]byte(vaultPassScript),
		executablePerm) // #nosec G306
	if err != nil {
		return fmt.Errorf("failed to write vault_pass script: %v", err)
	}
	return nil
}

func OpenURLInBrowser(url string) error {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "darwin":
		cmd = exec.Command("open", url)
	case "windows":
		cmd = exec.Command("rundll32", "url.dll,FileProtocolHandler", url)
	default: // Linux and others
		cmd = exec.Command("xdg-open", url)
	}

	return cmd.Run()
}

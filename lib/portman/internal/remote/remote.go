// Package remote runs portman commands on another host over ssh: it
// copies the local portman binary to the target and executes it there
// with the given arguments, so `portman --host NAME up` behaves like
// running `sudo portman up` after `ssh NAME`.
//
// All of that takes several ssh invocations (copy, run, clean up), but
// the user should only ever authenticate once. So the first thing Run
// does is open a master connection with stdio attached - any key
// passphrase, password, or 2FA prompt happens there, in the terminal -
// and every later invocation rides that same connection through ssh's
// control socket instead of authenticating again.
package remote

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// Run copies the currently running portman binary to host (an ssh
// destination, e.g. an entry in ~/.ssh/config) and executes it there
// with args, connecting stdio so interactive prompts (like a sudo
// password) work as they would locally.
func Run(host string, args []string) error {
	self, err := os.Executable()
	if err != nil {
		return fmt.Errorf("locate portman executable: %w", err)
	}
	if resolved, err := filepath.EvalSymlinks(self); err == nil {
		self = resolved
	}

	s, err := dial(host)
	if err != nil {
		return err
	}
	defer s.close()

	binary, err := s.copyBinary(self)
	if err != nil {
		return fmt.Errorf("copy portman to %s: %w", host, err)
	}
	defer s.removeAll(filepath.Dir(binary))

	if err := s.exec(binary, args); err != nil {
		return fmt.Errorf("run on %s: %w", host, err)
	}
	return nil
}

// session is a single authenticated ssh connection to a host, shared by
// every command Run needs to issue there.
type session struct {
	host string
	dir  string // local temp dir holding the control socket
	ctl  string // control socket path
}

// dial opens the master connection. It runs in the foreground until
// authentication finishes (so prompts reach the terminal), then puts
// itself in the background with no remote command (-N -f) and waits for
// commands to be multiplexed onto it.
func dial(host string) (*session, error) {
	dir, err := os.MkdirTemp("", "portman-ssh-")
	if err != nil {
		return nil, fmt.Errorf("create control directory: %w", err)
	}
	s := &session{host: host, dir: dir, ctl: filepath.Join(dir, "ctl")}

	cmd := exec.Command("ssh",
		"-o", "ControlMaster=yes",
		"-o", "ControlPath="+s.ctl,
		"-o", "ControlPersist=60",
		"-N", "-f", host)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	if err := cmd.Run(); err != nil {
		os.RemoveAll(dir)
		return nil, fmt.Errorf("connect to %s: %w", host, err)
	}
	return s, nil
}

// ssh builds an ssh invocation that reuses the master connection rather
// than opening (and authenticating) a new one. opts go before the host
// and remote (the command to run there, if any) after it, which is the
// order ssh requires - anything preceding the destination that is not a
// recognized option is taken as the destination itself.
func (s *session) ssh(opts []string, remote ...string) *exec.Cmd {
	args := append([]string{
		"-o", "ControlMaster=no",
		"-o", "ControlPath=" + s.ctl,
	}, opts...)
	args = append(args, s.host)
	return exec.Command("ssh", append(args, remote...)...)
}

// close tears down the master connection and the local socket directory.
func (s *session) close() {
	_ = s.ssh([]string{"-O", "exit"}).Run()
	_ = os.RemoveAll(s.dir)
}

// copyBinary streams the local binary over ssh's stdin into a fresh
// private directory on the host, and returns its remote path. The
// directory is created by mktemp rather than at a fixed path because the
// binary is about to be run under sudo: a predictable name under /tmp
// could be pre-created or symlinked by another local user.
func (s *session) copyBinary(localPath string) (string, error) {
	f, err := os.Open(localPath)
	if err != nil {
		return "", fmt.Errorf("open %s: %w", localPath, err)
	}
	defer f.Close()

	cmd := s.ssh(nil, `d=$(mktemp -d "${TMPDIR:-/tmp}/portman.XXXXXXXX") && `+
		`cat > "$d/portman" && chmod 0700 "$d/portman" && printf %s "$d"`)
	cmd.Stdin = f
	cmd.Stderr = os.Stderr
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	dir := strings.TrimSpace(string(out))
	if dir == "" {
		return "", fmt.Errorf("remote did not report a staging directory")
	}
	return dir + "/portman", nil
}

func (s *session) removeAll(dir string) {
	if dir == "" || dir == "/" {
		return
	}
	_ = s.ssh(nil, "rm -rf "+shellQuote(dir)).Run()
}

// exec runs the staged binary on the host. Commands that touch
// WireGuard, iptables, or /etc need root, so unless the ssh login is
// already root they go through sudo; -t allocates a tty, which is what
// lets sudo prompt for a password here on the local terminal.
func (s *session) exec(binary string, args []string) error {
	remote := shellQuote(binary) + " " + quoteArgs(args)
	if needsRoot(args) {
		remote = fmt.Sprintf(`if [ "$(id -u)" -eq 0 ]; then exec %s; else exec sudo -- %s; fi`, remote, remote)
	}

	cmd := s.ssh([]string{"-t"}, remote)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}

// needsRoot reports whether a portman subcommand has to run as root on
// the target. Nearly everything does: even the read-only commands parse
// the config, which lives root-owned and mode 0600. Only the ones that
// touch no state at all are exempt.
func needsRoot(args []string) bool {
	if len(args) == 0 {
		return false
	}
	switch args[0] {
	case "version", "-v", "--version", "help", "-h", "--help":
		return false
	default:
		return true
	}
}

// quoteArgs joins args into a single POSIX shell command line, since ssh
// concatenates a multi-argument command with spaces and hands it to the
// remote user's shell.
func quoteArgs(args []string) string {
	quoted := make([]string, len(args))
	for i, a := range args {
		quoted[i] = shellQuote(a)
	}
	return strings.Join(quoted, " ")
}

func shellQuote(s string) string {
	return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'"
}

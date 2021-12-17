package cli

import (
	"context"
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/cli/browser"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/cli/safeexec"
	"github.com/google/shlex"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/kittycad.go"
)

// CLI is the main type for the kittycad command line interface.
type CLI struct {
	IOStreams *iostreams.IOStreams
	Browser   Browser

	Context context.Context

	KittyCADClient func(string) (*kittycad.Client, error)
	Config         func() (config.Config, error)

	// Executable is the path to the currently invoked kittycad binary
	Executable func() string
}

// Browser represents the browser that kittycad will use to open links.
type Browser interface {
	Browse(string) error
}

// New returns a new CLI instance.
func New(ctx context.Context) *CLI {
	var exe string
	cli := &CLI{
		Context: ctx,
		Config:  configFunc(),
		Executable: func() string {
			if exe != "" {
				return exe
			}
			exe = executable("kittycad")
			return exe
		},
	}

	cli.IOStreams = ioStreams(cli)               // Depends on Config
	cli.KittyCADClient = kittycadClientFunc(cli) // Depends on Config
	cli.Browser = dobrowser(cli)                 // Depends on Config, and IOStreams

	return cli
}

func kittycadClientFunc(cli *CLI) func(string) (*kittycad.Client, error) {
	return func(hostname string) (*kittycad.Client, error) {
		cfg, err := cli.Config()
		if err != nil {
			return nil, err
		}
		return NewKittyCADClient(cfg, hostname)
	}
}

func dobrowser(cli *CLI) Browser {
	io := cli.IOStreams
	return NewBrowser(browserLauncher(cli), io.Out, io.ErrOut)
}

// NewBrowser creates a new Browser instance.
func NewBrowser(launcher string, stdout, stderr io.Writer) Browser {
	return &webBrowser{
		launcher: launcher,
		stdout:   stdout,
		stderr:   stderr,
	}
}

type webBrowser struct {
	launcher string
	stdout   io.Writer
	stderr   io.Writer
}

// Browse opens the given URL in the default browser of the user.
func (b *webBrowser) Browse(url string) error {
	if b.launcher != "" {
		launcherArgs, err := shlex.Split(b.launcher)
		if err != nil {
			return err
		}
		launcherExe, err := safeexec.LookPath(launcherArgs[0])
		if err != nil {
			return err
		}
		args := append(launcherArgs[1:], url)
		cmd := exec.Command(launcherExe, args...)
		cmd.Stdout = b.stdout
		cmd.Stderr = b.stderr
		return cmd.Run()
	}

	return browser.OpenURL(url)
}

// Browser precedence
// 1. GH_BROWSER
// 2. browser from config
// 3. BROWSER
func browserLauncher(cli *CLI) string {
	if kittycadBrowser := os.Getenv("KITTYCAD_BROWSER"); kittycadBrowser != "" {
		return kittycadBrowser
	}

	cfg, err := cli.Config()
	if err == nil {
		if cfgBrowser, _ := cfg.Get("", "browser"); cfgBrowser != "" {
			return cfgBrowser
		}
	}

	return os.Getenv("BROWSER")
}

// Finds the location of the executable for the current process as it's found in PATH, respecting symlinks.
// If the process couldn't determine its location, return fallbackName. If the executable wasn't found in
// PATH, return the absolute location to the program.
//
// The idea is that the result of this function is callable in the future and refers to the same
// installation of kittycad, even across upgrades. This is needed primarily for Homebrew, which installs software
// under a location such as `/usr/local/Cellar/kittycad/1.13.1/bin/kittycad` and symlinks it from `/usr/local/bin/kittycad`.
// When the version is upgraded, Homebrew will often delete older versions, but keep the symlink. Because of
// this, we want to refer to the `kittycad` binary as `/usr/local/bin/kittycad` and not as its internal Homebrew
// location.
//
// None of this would be needed if we could just refer to KittyCAD CLI as `kittycad`, i.e. without using an absolute
// path. However, for some reason Homebrew does not include `/usr/local/bin` in PATH when it invokes git
// commands to update its taps.
func executable(fallbackName string) string {
	exe, err := os.Executable()
	if err != nil {
		return fallbackName
	}

	base := filepath.Base(exe)
	path := os.Getenv("PATH")
	for _, dir := range filepath.SplitList(path) {
		p, err := filepath.Abs(filepath.Join(dir, base))
		if err != nil {
			continue
		}
		f, err := os.Stat(p)
		if err != nil {
			continue
		}

		if p == exe {
			return p
		} else if f.Mode()&os.ModeSymlink != 0 {
			if t, err := os.Readlink(p); err == nil && t == exe {
				return p
			}
		}
	}

	return exe
}

func configFunc() func() (config.Config, error) {
	var cachedConfig config.Config
	var configError error
	return func() (config.Config, error) {
		if cachedConfig != nil || configError != nil {
			return cachedConfig, configError
		}
		cachedConfig, configError = config.ParseDefaultConfig()
		if errors.Is(configError, os.ErrNotExist) {
			cachedConfig = config.NewBlankConfig()
			configError = nil
		}
		cachedConfig = config.InheritEnv(cachedConfig)
		return cachedConfig, configError
	}
}

func ioStreams(cli *CLI) *iostreams.IOStreams {
	io := iostreams.System()
	cfg, err := cli.Config()
	if err != nil {
		return io
	}

	if prompt, _ := cfg.Get("", "prompt"); prompt == "disabled" {
		io.SetNeverPrompt(true)
	}

	// Pager precedence
	// 1. GH_PAGER
	// 2. pager from config
	// 3. PAGER
	if ghPager, ghPagerExists := os.LookupEnv("KITTYCAD_PAGER"); ghPagerExists {
		io.SetPager(ghPager)
	} else if pager, _ := cfg.Get("", "pager"); pager != "" {
		io.SetPager(pager)
	}

	return io
}

package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/AlecAivazis/survey/v2/core"
	"github.com/AlecAivazis/survey/v2/terminal"
	"github.com/cli/safeexec"
	"github.com/google/go-github/github"
	"github.com/kittycad/cli/cmd/root"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/internal/run"
	"github.com/kittycad/cli/internal/update"
	"github.com/kittycad/cli/kittycad"
	"github.com/kittycad/cli/pkg/aliases/expand"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/kittycad/cli/pkg/cmdutil"
	"github.com/kittycad/cli/version"
	"github.com/mgutz/ansi"
	"github.com/spf13/cobra"
)

// TODO: Go back to this version when everything is open source.
//"github.com/kittycad/kittycad.go"

/*var kittycadClient *kittycad.Client
kittycadClient, err = kittycad.NewClientFromEnv()
if err != nil {
	return err
}*/

type exitCode int

const (
	exitOK     exitCode = 0
	exitError  exitCode = 1
	exitCancel exitCode = 2
	exitAuth   exitCode = 4
)

func main() {
	code := mainRun()
	os.Exit(int(code))
}

func mainRun() exitCode {
	ctx := context.Background()
	updateMessageChan := make(chan *github.RepositoryRelease)
	go func() {
		rel, _ := checkForUpdate(ctx, version.VERSION)
		updateMessageChan <- rel
	}()

	hasDebug := os.Getenv("DEBUG") != ""

	cli := cli.New(ctx)
	stderr := cli.IOStreams.ErrOut

	if spec := os.Getenv("KITTYCAD_FORCE_TTY"); spec != "" {
		cli.IOStreams.ForceTerminal(spec)
	}
	if !cli.IOStreams.ColorEnabled() {
		core.DisableColor = true
	} else {
		// override survey's poor choice of color
		core.TemplateFuncsWithColor["color"] = func(style string) string {
			switch style {
			case "white":
				if cli.IOStreams.ColorSupport256() {
					return fmt.Sprintf("\x1b[%d;5;%dm", 38, 242)
				}
				return ansi.ColorCode("default")
			default:
				return ansi.ColorCode(style)
			}
		}
	}

	// Enable running gh from Windows File Explorer's address bar. Without this, the user is told to stop and run from a
	// terminal. With this, a user can clone a repo (or take other actions) directly from explorer.
	if len(os.Args) > 1 && os.Args[1] != "" {
		cobra.MousetrapHelpText = ""
	}

	rootCmd := root.NewCmdRoot(cli)

	cfg, err := cli.Config()
	if err != nil {
		fmt.Fprintf(stderr, "failed to read configuration:  %s\n", err)
		return exitError
	}

	expandedArgs := []string{}
	if len(os.Args) > 0 {
		expandedArgs = os.Args[1:]
	}

	// translate `kittycad help <command>` to `kittycad <command> --help` for extensions
	if len(expandedArgs) == 2 && expandedArgs[0] == "help" && !hasCommand(rootCmd, expandedArgs[1:]) {
		expandedArgs = []string{expandedArgs[1], "--help"}
	}
	if !hasCommand(rootCmd, expandedArgs) {
		originalArgs := expandedArgs
		isShell := false

		argsForExpansion := append([]string{"kittycad"}, expandedArgs...)
		expandedArgs, isShell, err = expand.ExpandAlias(cfg, argsForExpansion, nil)
		if err != nil {
			fmt.Fprintf(stderr, "failed to process aliases:  %s\n", err)
			return exitError
		}

		if hasDebug {
			fmt.Fprintf(stderr, "%v -> %v\n", originalArgs, expandedArgs)
		}

		if isShell {
			exe, err := safeexec.LookPath(expandedArgs[0])
			if err != nil {
				fmt.Fprintf(stderr, "failed to run external command: %s", err)
				return exitError
			}

			externalCmd := exec.Command(exe, expandedArgs[1:]...)
			externalCmd.Stderr = os.Stderr
			externalCmd.Stdout = os.Stdout
			externalCmd.Stdin = os.Stdin
			preparedCmd := run.PrepareCmd(externalCmd)

			err = preparedCmd.Run()
			if err != nil {
				var execError *exec.ExitError
				if errors.As(err, &execError) {
					return exitCode(execError.ExitCode())
				}
				fmt.Fprintf(stderr, "failed to run external command: %s", err)
				return exitError
			}

			return exitOK
		} else if len(expandedArgs) > 0 && !hasCommand(rootCmd, expandedArgs) {
			// If we had extensions, we would check for them here.
			return exitError
		}
	}

	// provide completions for aliases and extensions
	rootCmd.ValidArgsFunction = func(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
		var results []string
		if aliases, err := cfg.Aliases(); err == nil {
			for aliasName := range aliases.All() {
				if strings.HasPrefix(aliasName, toComplete) {
					results = append(results, aliasName)
				}
			}
		}
		return results, cobra.ShellCompDirectiveNoFileComp
	}

	cs := cli.IOStreams.ColorScheme()

	authError := errors.New("authError")
	rootCmd.PersistentPreRunE = func(cmd *cobra.Command, args []string) error {
		// require that the user is authenticated before running most commands
		if cmdutil.IsAuthCheckEnabled(cmd) && !cmdutil.CheckAuth(cfg) {
			fmt.Fprintln(stderr, cs.Bold("Welcome to KittyCAD CLI!"))
			fmt.Fprintln(stderr)
			fmt.Fprintln(stderr, "To authenticate, please run `kittycad auth login`.")
			return authError
		}

		return nil
	}

	rootCmd.SetArgs(expandedArgs)

	if cmd, err := rootCmd.ExecuteC(); err != nil {
		if err == cmdutil.SilentError {
			return exitError
		} else if cmdutil.IsUserCancellation(err) {
			if errors.Is(err, terminal.InterruptErr) {
				// ensure the next shell prompt will start on its own line
				fmt.Fprint(stderr, "\n")
			}
			return exitCancel
		} else if errors.Is(err, authError) {
			return exitAuth
		}

		printError(stderr, err, cmd, hasDebug)

		if strings.Contains(err.Error(), "Incorrect function") {
			fmt.Fprintln(stderr, "You appear to be running in MinTTY without pseudo terminal support.")
			fmt.Fprintln(stderr, "To learn about workarounds for this error, run:  kittycad help mintty")
			return exitError
		}

		var httpErr kittycad.HTTPError
		if errors.As(err, &httpErr) && (httpErr.StatusCode >= 401 && httpErr.StatusCode < 500) {
			fmt.Fprintln(stderr, "Try authenticating with:  kittycad auth login")
		}

		return exitError
	}
	if root.HasFailed() {
		return exitError
	}

	newRelease := <-updateMessageChan
	if newRelease != nil {
		isHomebrew := isUnderHomebrew(cli.Executable())
		if isHomebrew && isRecentRelease(newRelease.PublishedAt.Time) {
			// do not notify Homebrew users before the version bump had a chance to get merged into homebrew-core
			return exitOK
		}
		fmt.Fprintf(stderr, "\n\n%s %s → %s\n",
			ansi.Color("A new release of kittycad is available:", "yellow"),
			ansi.Color(version.VERSION, "cyan"),
			ansi.Color(*newRelease.TagName, "cyan"))
		if isHomebrew {
			fmt.Fprintf(stderr, "To upgrade, run: %s\n", "brew update && brew upgrade kittycad")
		}
		fmt.Fprintf(stderr, "%s\n\n",
			ansi.Color(*newRelease.URL, "yellow"))
	}

	return exitOK
}

// hasCommand returns true if args resolve to a built-in command
func hasCommand(rootCmd *cobra.Command, args []string) bool {
	c, _, err := rootCmd.Traverse(args)
	return err == nil && c != rootCmd
}

func printError(out io.Writer, err error, cmd *cobra.Command, debug bool) {
	var dnsError *net.DNSError
	if errors.As(err, &dnsError) {
		fmt.Fprintf(out, "error connecting to %s\n", dnsError.Name)
		if debug {
			fmt.Fprintln(out, dnsError)
		}
		fmt.Fprintln(out, "check your internet connection or https://status.kittycad.io")
		return
	}

	fmt.Fprintln(out, err)

	var flagError *cmdutil.FlagError
	if errors.As(err, &flagError) || strings.HasPrefix(err.Error(), "unknown command ") {
		if !strings.HasSuffix(err.Error(), "\n") {
			fmt.Fprintln(out)
		}
		fmt.Fprintln(out, cmd.UsageString())
	}
}

func shouldCheckForUpdate() bool {
	if os.Getenv("KITTYCAD_NO_UPDATE_NOTIFIER") != "" {
		return false
	}
	return !isCI() && cmdutil.IsTerminal(os.Stdout) && cmdutil.IsTerminal(os.Stderr)
}

// based on https://github.com/watson/ci-info/blob/HEAD/index.js
func isCI() bool {
	return os.Getenv("CI") != "" || // GitHub Actions, Travis CI, CircleCI, Cirrus CI, GitLab CI, AppVeyor, CodeShip, dsari
		os.Getenv("BUILD_NUMBER") != "" || // Jenkins, TeamCity
		os.Getenv("RUN_ID") != "" // TaskCluster, dsari
}

func checkForUpdate(ctx context.Context, currentVersion string) (*github.RepositoryRelease, error) {
	if !shouldCheckForUpdate() {
		return nil, nil
	}

	stateFilePath := filepath.Join(config.StateDir(), "state.yml")
	return update.CheckForUpdate(ctx, stateFilePath, "kittycad", "cli", currentVersion)
}

func isRecentRelease(publishedAt time.Time) bool {
	return !publishedAt.IsZero() && time.Since(publishedAt) < time.Hour*24
}

// Check whether the gh binary was found under the Homebrew prefix
func isUnderHomebrew(ghBinary string) bool {
	brewExe, err := safeexec.LookPath("brew")
	if err != nil {
		return false
	}

	brewPrefixBytes, err := exec.Command(brewExe, "--prefix").Output()
	if err != nil {
		return false
	}

	brewBinPrefix := filepath.Join(strings.TrimSpace(string(brewPrefixBytes)), "bin") + string(filepath.Separator)
	return strings.HasPrefix(ghBinary, brewBinPrefix)
}

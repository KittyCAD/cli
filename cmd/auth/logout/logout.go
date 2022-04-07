package logout

import (
	"context"
	"errors"
	"fmt"

	"github.com/AlecAivazis/survey/v2"
	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/cli/cli/v2/pkg/prompt"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/kittycad/cli/pkg/cmdutil"
	"github.com/kittycad/kittycad.go"
	"github.com/spf13/cobra"
)

// Options the options for the logout command.
type Options struct {
	KittyCADClient func(string) (*kittycad.Client, error)
	IO             *iostreams.IOStreams
	Config         func() (config.Config, error)
	Context        context.Context

	Hostname string
}

// NewCmdLogout creates a new `kittycad auth logout` command.
func NewCmdLogout(cli *cli.CLI, runF func(*Options) error) *cobra.Command {
	opts := &Options{
		KittyCADClient: cli.KittyCADClient,
		IO:             cli.IOStreams,
		Config:         cli.Config,
		Context:        cli.Context,
	}

	cmd := &cobra.Command{
		Use:   "logout",
		Args:  cobra.ExactArgs(0),
		Short: "Log out of a KittyCAD host",
		Long: heredoc.Doc(`Remove authentication for a KittyCAD host.

			This command removes the authentication configuration for a host either specified
			interactively or via --hostname.
		`),
		Example: heredoc.Doc(`
			$ kittycad auth logout
			# => select what host to log out of via a prompt

			$ kittycad auth logout --hostname kittycad.internal
			# => log out of specified host
		`),
		RunE: func(cmd *cobra.Command, args []string) error {
			if opts.Hostname == "" && !opts.IO.CanPrompt() {
				return cmdutil.FlagErrorf("--hostname required when not running interactively")
			}

			if runF != nil {
				return runF(opts)
			}

			return logoutRun(opts)
		},
	}

	cmd.Flags().StringVarP(&opts.Hostname, "hostname", "h", "", "The hostname of the KittyCAD instance to log out of")

	return cmd
}

func logoutRun(opts *Options) error {
	hostname := opts.Hostname

	cfg, err := opts.Config()
	if err != nil {
		return err
	}

	candidates, err := cfg.Hosts()
	if err != nil {
		return err
	}
	if len(candidates) == 0 {
		return fmt.Errorf("not logged in to any hosts")
	}

	if hostname == "" {
		if len(candidates) == 1 {
			hostname = candidates[0]
		} else {
			err = prompt.SurveyAskOne(&survey.Select{
				Message: "What account do you want to log out of?",
				Options: candidates,
			}, &hostname)

			if err != nil {
				return fmt.Errorf("could not prompt: %w", err)
			}
		}
	} else {
		var found bool
		for _, c := range candidates {
			if c == hostname {
				found = true
				break
			}
		}

		if !found {
			return fmt.Errorf("not logged into %s", hostname)
		}
	}

	if err := cfg.CheckWriteable(hostname, "token"); err != nil {
		var roErr *config.ReadOnlyEnvError
		if errors.As(err, &roErr) {
			fmt.Fprintf(opts.IO.ErrOut, "The value of the %s environment variable is being used for authentication.\n", roErr.Variable)
			fmt.Fprint(opts.IO.ErrOut, "To erase credentials stored in KittyCAD CLI, first clear the value from the environment.\n")
			return cmdutil.ErrSilent
		}
		return err
	}

	kittycadClient, err := opts.KittyCADClient(hostname)
	if err != nil {
		return err
	}

	session, err := kittycadClient.User.GetSelf()
	if err != nil {
		return err
	}

	usernameStr := ""
	if session.Email != "" {
		usernameStr = fmt.Sprintf(" account '%s'", session.Email)
	}

	if opts.IO.CanPrompt() {
		var keepGoing bool
		err := prompt.SurveyAskOne(&survey.Confirm{
			Message: fmt.Sprintf("Are you sure you want to log out of %s%s?", hostname, usernameStr),
			Default: true,
		}, &keepGoing)
		if err != nil {
			return fmt.Errorf("could not prompt: %w", err)
		}

		if !keepGoing {
			return nil
		}
	}

	cfg.UnsetHost(hostname)
	err = cfg.Write()
	if err != nil {
		return fmt.Errorf("failed to write config, authentication configuration not updated: %w", err)
	}

	isTTY := opts.IO.IsStdinTTY() && opts.IO.IsStdoutTTY()

	if isTTY {
		cs := opts.IO.ColorScheme()
		fmt.Fprintf(opts.IO.ErrOut, "%s Logged out of %s%s\n",
			cs.SuccessIcon(), cs.Bold(hostname), usernameStr)
	}

	return nil
}

package login

import (
	"context"
	"errors"
	"fmt"
	"io/ioutil"
	"strings"

	"github.com/AlecAivazis/survey/v2"
	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/cli/cli/v2/pkg/prompt"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/kittycad"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/kittycad/cli/pkg/cmdutil"
	"github.com/spf13/cobra"
)

// Options are the options for `kittycad auth login`.
type Options struct {
	IO             *iostreams.IOStreams
	Config         func() (config.Config, error)
	KittyCADClient func() (*kittycad.Client, error)
	Context        context.Context

	MainExecutable string

	Interactive bool

	Hostname string
	Token    string
}

// NewCmdLogin creates a new `kittycad auth login` command.
func NewCmdLogin(cli *cli.CLI, runF func(*Options) error) *cobra.Command {
	opts := &Options{
		IO:             cli.IOStreams,
		Config:         cli.Config,
		KittyCADClient: cli.KittyCADClient,
		Context:        cli.Context,
	}

	var tokenStdin bool

	cmd := &cobra.Command{
		Use:   "login",
		Args:  cobra.ExactArgs(0),
		Short: "Authenticate with a KittyCAD host",
		Long: heredoc.Docf(`
			Authenticate with a KittyCAD host.

			Alternatively, pass in a token on standard input by using %[1]s--with-token%[1]s.
		`, "`"),
		Example: heredoc.Doc(`
			# start interactive setup
			$ kittycad auth login

			# authenticate against kittycad.io by reading the token from a file
			$ kittycad auth login --with-token < mytoken.txt

			# authenticate with a specific KittyCAD Server instance
			$ kittycad auth login --hostname kittycad.internal
		`),
		RunE: func(cmd *cobra.Command, args []string) error {
			if !opts.IO.CanPrompt() && !tokenStdin {
				return cmdutil.FlagErrorf("--with-token required when not running interactively")
			}

			if tokenStdin {
				defer opts.IO.In.Close()
				token, err := ioutil.ReadAll(opts.IO.In)
				if err != nil {
					return fmt.Errorf("failed to read token from STDIN: %w", err)
				}
				opts.Token = strings.TrimSpace(string(token))
			}

			if opts.IO.CanPrompt() && opts.Token == "" {
				opts.Interactive = true
			}

			if cmd.Flags().Changed("hostname") {
				if err := cmdutil.HostnameValidator(opts.Hostname); err != nil {
					return cmdutil.FlagErrorf("error parsing --hostname: %w", err)
				}
			}

			if !opts.Interactive {
				if opts.Hostname == "" {
					opts.Hostname = config.KittyCADDefaultHost
				}
			}

			opts.MainExecutable = cli.Executable()

			if runF != nil {
				return runF(opts)
			}

			return loginRun(opts)
		},
	}

	cmd.Flags().StringVarP(&opts.Hostname, "hostname", "h", "", "The hostname of the KittyCAD instance to authenticate with")
	cmd.Flags().BoolVar(&tokenStdin, "with-token", false, "Read token from standard input")
	//TODO: support auth through browser
	//cmd.Flags().BoolVarP(&opts.Web, "web", "w", false, "Open a browser to authenticate")

	return cmd
}

func loginRun(opts *Options) error {
	cfg, err := opts.Config()
	if err != nil {
		return err
	}

	hostname := opts.Hostname
	if hostname == "" {
		if opts.Interactive {
			var err error
			hostname, err = promptForHostname()
			if err != nil {
				return err
			}
		} else {
			return errors.New("must specify --hostname")
		}
	}

	if err := cfg.CheckWriteable(hostname, "token"); err != nil {
		var roErr *config.ReadOnlyEnvError
		if errors.As(err, &roErr) {
			fmt.Fprintf(opts.IO.ErrOut, "The value of the %s environment variable is being used for authentication.\n", roErr.Variable)
			fmt.Fprint(opts.IO.ErrOut, "To have KittyCAD CLI store credentials instead, first clear the value from the environment.\n")
			return cmdutil.ErrSilent
		}
		return err
	}

	if opts.Token != "" {
		err := cfg.Set(hostname, "token", opts.Token)
		if err != nil {
			return err
		}

		return cfg.Write()
	}

	existingToken, _ := cfg.Get(hostname, "token")
	if existingToken != "" && opts.Interactive {
		var keepGoing bool
		err = prompt.SurveyAskOne(&survey.Confirm{
			Message: fmt.Sprintf(
				"You're already logged into %s. Do you want to re-authenticate?",
				hostname),
			Default: false,
		}, &keepGoing)
		if err != nil {
			return fmt.Errorf("could not prompt: %w", err)
		}
		if !keepGoing {
			return nil
		}
	}

	return Flow(&FlowOptions{
		IO:             opts.IO,
		Config:         cfg,
		KittyCADClient: opts.KittyCADClient,
		Hostname:       hostname,
		Interactive:    opts.Interactive,
		Executable:     opts.MainExecutable,
		Context:        opts.Context,
	})
}

func promptForHostname() (string, error) {
	var hostType int
	err := prompt.SurveyAskOne(&survey.Select{
		Message: "What account do you want to log into?",
		Options: []string{
			"kittycad.io",
			"Other KittyCAD Server",
		},
	}, &hostType)

	if err != nil {
		return "", fmt.Errorf("could not prompt: %w", err)
	}

	isOtherServer := hostType == 1

	hostname := config.KittyCADDefaultHost
	if isOtherServer {
		err := prompt.SurveyAskOne(&survey.Input{
			Message: "KittyCAD server hostname:",
		}, &hostname, survey.WithValidator(cmdutil.HostnameValidator))
		if err != nil {
			return "", fmt.Errorf("could not prompt: %w", err)
		}
	}

	return hostname, nil
}

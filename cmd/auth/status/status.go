package status

import (
	"context"
	"fmt"

	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/kittycad"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/kittycad/cli/pkg/cmdutil"
	"github.com/spf13/cobra"
)

// Options are options for the `kittycad auth status` command.
type Options struct {
	KittyCADClient func() (*kittycad.Client, error)
	IO             *iostreams.IOStreams
	Config         func() (config.Config, error)
	Context        context.Context

	Hostname  string
	ShowToken bool
}

// NewCmdStatus creates a new `kittycad auth status` command.
func NewCmdStatus(cli *cli.CLI, runF func(*Options) error) *cobra.Command {
	opts := &Options{
		KittyCADClient: cli.KittyCADClient,
		IO:             cli.IOStreams,
		Config:         cli.Config,
		Context:        cli.Context,
	}

	cmd := &cobra.Command{
		Use:   "status",
		Args:  cobra.ExactArgs(0),
		Short: "View authentication status",
		Long: heredoc.Doc(`Verifies and displays information about your authentication state.

			This command will test your authentication state for each KittyCAD host that kittycad
			knows about and report on any issues.
		`),
		RunE: func(cmd *cobra.Command, args []string) error {
			if runF != nil {
				return runF(opts)
			}

			return statusRun(opts)
		},
	}

	cmd.Flags().StringVarP(&opts.Hostname, "hostname", "h", "", "Check a specific hostname's auth status")
	cmd.Flags().BoolVarP(&opts.ShowToken, "show-token", "t", false, "Display the auth token")

	return cmd
}

func statusRun(opts *Options) error {
	cfg, err := opts.Config()
	if err != nil {
		return err
	}

	// TODO check tty

	stderr := opts.IO.ErrOut

	cs := opts.IO.ColorScheme()

	statusInfo := map[string][]string{}

	hostnames, err := cfg.Hosts()
	if err != nil {
		return err
	}
	if len(hostnames) == 0 {
		fmt.Fprintf(stderr,
			"You are not logged into any KittyCAD hosts. Run %s to authenticate.\n", cs.Bold("kittycad auth login"))
		return cmdutil.ErrSilent
	}

	kittycadClient, err := opts.KittyCADClient()
	if err != nil {
		return err
	}

	var failed bool
	var isHostnameFound bool

	for _, hostname := range hostnames {
		if opts.Hostname != "" && opts.Hostname != hostname {
			continue
		}
		isHostnameFound = true

		token, tokenSource, _ := cfg.GetWithSource(hostname, "token")

		statusInfo[hostname] = []string{}
		addMsg := func(x string, ys ...interface{}) {
			statusInfo[hostname] = append(statusInfo[hostname], fmt.Sprintf(x, ys...))
		}

		session, err := kittycadClient.MetaDebugSession(opts.Context)
		if err != nil {
			addMsg("%s %s: api call failed: %s", cs.Red("X"), hostname, err)
		}

		// Let the user know if their token is invalid.
		if !session.IsValid {
			addMsg("%s Logged in to %s as %s (%s) with an invalid token", cs.Red("X"), hostname, cs.Bold(*session.UserId), tokenSource)
			failed = true
			continue
		}

		// TODO: get the user's email in the session.
		addMsg("%s Logged in to %s as %s (%s)", cs.SuccessIcon(), hostname, cs.Bold(*session.UserId), tokenSource)
		tokenDisplay := "*******************"
		if opts.ShowToken {
			tokenDisplay = token
		}
		addMsg("%s Token: %s", cs.SuccessIcon(), tokenDisplay)
		addMsg("")
	}

	if !isHostnameFound {
		fmt.Fprintf(stderr,
			"Hostname %q not found among authenticated KittyCAD hosts\n", opts.Hostname)
		return cmdutil.ErrSilent
	}

	for _, hostname := range hostnames {
		lines, ok := statusInfo[hostname]
		if !ok {
			continue
		}
		fmt.Fprintf(stderr, "%s\n", cs.Bold(hostname))
		for _, line := range lines {
			fmt.Fprintf(stderr, "  %s\n", line)
		}
	}

	if failed {
		return cmdutil.ErrSilent
	}

	return nil
}

package status

import (
	"context"

	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/kittycad/cli/kittycad"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

// Options defines the options of the `file stattus` command.
type Options struct {
	IO             *iostreams.IOStreams
	KittyCADClient func() (*kittycad.Client, error)
	Context        context.Context

	// Flag options.
	ID string
}

// NewCmdStatus returns a new instance of the status command.
func NewCmdStatus(cli *cli.CLI, runF func(*Options) error) *cobra.Command {
	opts := &Options{
		IO:             cli.IOStreams,
		KittyCADClient: cli.KittyCADClient,
		Context:        cli.Context,
	}

	cmd := &cobra.Command{
		Use:   "status <id>",
		Short: "Get a file conversion",
		Long:  `Get the status of a file conversion`,
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) > 0 {
				opts.ID = args[0]
			}

			if runF != nil {
				return runF(opts)
			}

			return nil
		},
	}

	return cmd
}

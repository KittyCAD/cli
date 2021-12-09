package delete

import (
	"fmt"

	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

// Options are options for deleting an alias.
type Options struct {
	Config func() (config.Config, error)
	IO     *iostreams.IOStreams

	Name string
}

// NewCmdDelete creates a new `delete` subcommand.
func NewCmdDelete(cli *cli.CLI, runF func(*Options) error) *cobra.Command {
	opts := &Options{
		IO:     cli.IOStreams,
		Config: cli.Config,
	}

	cmd := &cobra.Command{
		Use:   "delete <alias>",
		Short: "Delete an alias",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			opts.Name = args[0]

			if runF != nil {
				return runF(opts)
			}
			return deleteRun(opts)
		},
	}

	return cmd
}

func deleteRun(opts *Options) error {
	cfg, err := opts.Config()
	if err != nil {
		return err
	}

	aliasCfg, err := cfg.Aliases()
	if err != nil {
		return fmt.Errorf("couldn't read aliases config: %w", err)
	}

	expansion, ok := aliasCfg.Get(opts.Name)
	if !ok {
		return fmt.Errorf("no such alias %s", opts.Name)

	}

	err = aliasCfg.Delete(opts.Name)
	if err != nil {
		return fmt.Errorf("failed to delete alias %s: %w", opts.Name, err)
	}

	if opts.IO.IsStdoutTTY() {
		cs := opts.IO.ColorScheme()
		fmt.Fprintf(opts.IO.ErrOut, "%s Deleted alias %s; was %s\n", cs.SuccessIconWithColor(cs.Red), opts.Name, expansion)
	}

	return nil
}

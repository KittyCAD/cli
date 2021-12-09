package list

import (
	"fmt"

	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

// Options defines the behavior of the list command.
type Options struct {
	IO     *iostreams.IOStreams
	Config func() (config.Config, error)

	Hostname string
}

// NewCmdConfigList creates a new config list command.
func NewCmdConfigList(cli *cli.CLI) *cobra.Command {
	opts := &Options{
		IO:     cli.IOStreams,
		Config: cli.Config,
	}

	cmd := &cobra.Command{
		Use:   "list",
		Short: "Print a list of configuration keys and values",
		Args:  cobra.ExactArgs(0),
		RunE: func(cmd *cobra.Command, args []string) error {
			return listRun(opts)
		},
	}

	cmd.Flags().StringVarP(&opts.Hostname, "host", "h", "", "Get per-host configuration")

	return cmd
}

func listRun(opts *Options) error {
	cfg, err := opts.Config()
	if err != nil {
		return err
	}

	var host string
	if opts.Hostname != "" {
		host = opts.Hostname
	} else {
		host, err = cfg.DefaultHost()
		if err != nil {
			return err
		}
	}

	configOptions := config.Options()

	for _, key := range configOptions {
		val, err := cfg.Get(host, key.Key)
		if err != nil {
			return err
		}
		fmt.Fprintf(opts.IO.Out, "%s=%s\n", key.Key, val)
	}

	return nil
}

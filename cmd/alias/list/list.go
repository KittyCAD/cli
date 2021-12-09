package list

import (
	"fmt"
	"sort"

	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/cli/cli/v2/utils"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

// Options encapsulates options for the `kittycad alias list` command.
type Options struct {
	Config func() (config.Config, error)
	IO     *iostreams.IOStreams
}

// NewCmdList creates a new `kittycad alias list` command.
func NewCmdList(cli *cli.CLI, runF func(*Options) error) *cobra.Command {
	opts := &Options{
		IO:     cli.IOStreams,
		Config: cli.Config,
	}

	cmd := &cobra.Command{
		Use:   "list",
		Short: "List your aliases",
		Long: heredoc.Doc(`
			This command prints out all of the aliases kittycad is configured to use.
		`),
		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			if runF != nil {
				return runF(opts)
			}
			return listRun(opts)
		},
	}

	return cmd
}

func listRun(opts *Options) error {
	cfg, err := opts.Config()
	if err != nil {
		return err
	}

	aliasCfg, err := cfg.Aliases()
	if err != nil {
		return fmt.Errorf("couldn't read aliases config: %w", err)
	}

	if aliasCfg.Empty() {
		if opts.IO.IsStdoutTTY() {
			fmt.Fprintf(opts.IO.ErrOut, "no aliases configured\n")
		}
		return nil
	}

	tp := utils.NewTablePrinter(opts.IO)

	aliasMap := aliasCfg.All()
	keys := []string{}
	for alias := range aliasMap {
		keys = append(keys, alias)
	}
	sort.Strings(keys)

	for _, alias := range keys {
		tp.AddField(alias+":", nil, nil)
		tp.AddField(aliasMap[alias], nil, nil)
		tp.EndRow()
	}

	return tp.Render()
}

package status

import (
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

// NewCmdStatus returns a new instance of the status command.
func NewCmdStatus(cli *cli.CLI) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "status <id>",
		Short: "Get a file conversion",
		Long:  `Get the status of a file conversion`,
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			_, err := cli.KittyCADClient()
			if err != nil {
				return err
			}

			return nil
		},
	}

	return cmd
}

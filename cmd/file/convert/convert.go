package convert

import (
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

// NewCmdConvert creates a new cobra.Command for the convert subcommand.
func NewCmdConvert(cli *cli.CLI) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "convert",
		Short: "Convert CAD file",
		Long:  `Convert a CAD file from one format to another. If the file being converted is larger than a certain size it will be performed asynchronously, you can then check its status with the status command`,
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

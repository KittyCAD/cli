package instance

import (
	"fmt"

	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

func NewCmdInstance(cli *cli.CLI) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "instance",
		Short: "Get instance metadata",
		Long:  `Get information about this specific API server instance. This is primarily used for debugging`,
		RunE: func(cmd *cobra.Command, args []string) error {
			kittycadClient, err := cli.KittyCADClient()
			if err != nil {
				return err
			}

			instance, err := kittycadClient.MetaDebugInstance(cli.Context)
			if err != nil {
				return fmt.Errorf("failed to get auth instance: %w", err)
			}

			fmt.Printf("%#v\n", instance)
			return nil
		},
	}

	return cmd
}

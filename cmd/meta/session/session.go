package session

import (
	"fmt"

	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

// NewCmdSession returns a new instance of the session command.
func NewCmdSession(cli *cli.CLI) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "session",
		Short: "Get auth session",
		Long:  `Get information about your API request session. This is primarily used for debugging`,
		RunE: func(cmd *cobra.Command, args []string) error {
			kittycadClient, err := cli.KittyCADClient()
			if err != nil {
				return err
			}

			session, err := kittycadClient.MetaDebugSession(cli.Context)
			if err != nil {
				return fmt.Errorf("failed to get auth session: %w", err)
			}

			fmt.Printf("%#v\n", session)
			return nil
		},
	}

	return cmd
}

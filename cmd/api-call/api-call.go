package file

import (
	"github.com/MakeNowJust/heredoc"
	cmdStatus "github.com/kittycad/cli/cmd/api-call/status"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

// NewCmdAPICall returns a new instance of the file command.
func NewCmdAPICall(cli *cli.CLI) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "api-call <command>",
		Short: "async API call operations",
		Long:  `Get the status of an async API calls and other operations.`,
		Example: heredoc.Doc(`
			# get the status of an asynchronous API call
			$ kittycad api-call status <uuid_of_api_call>
		`),
		Annotations: map[string]string{
			"IsCore": "true",
		},
	}

	cmd.AddCommand(cmdStatus.NewCmdStatus(cli, nil))

	return cmd
}

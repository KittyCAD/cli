package meta

import (
	"github.com/MakeNowJust/heredoc"
	cmdInstance "github.com/kittycad/cli/cmd/meta/instance"
	cmdSession "github.com/kittycad/cli/cmd/meta/session"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

func NewCmdMeta(cli *cli.CLI) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "meta <command>",
		Short: "Meta information",
		Long:  `Get information about sessions, servers, and instances. This is best used for debugging authentication sessions, etc`,
		Example: heredoc.Doc(`
			$ kittycad meta instance
			$ kittycad meta session
		`),
		Annotations: map[string]string{
			"IsCore": "true",
		},
	}

	cmd.AddCommand(cmdInstance.NewCmdInstance(cli))
	cmd.AddCommand(cmdSession.NewCmdSession(cli))

	return cmd
}

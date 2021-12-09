package alias

import (
	"github.com/MakeNowJust/heredoc"
	deleteCmd "github.com/kittycad/cli/cmd/alias/delete"
	listCmd "github.com/kittycad/cli/cmd/alias/list"
	setCmd "github.com/kittycad/cli/cmd/alias/set"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/kittycad/cli/pkg/cmdutil"
	"github.com/spf13/cobra"
)

// NewCmdAlias creates the alias command.
func NewCmdAlias(cli *cli.CLI) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "alias <command>",
		Short: "Create command shortcuts",
		Long: heredoc.Doc(`
			Aliases can be used to make shortcuts for kittycad commands or to compose multiple commands.

			Run "kittycad help alias set" to learn more.
		`),
	}

	cmdutil.DisableAuthCheck(cmd)

	cmd.AddCommand(deleteCmd.NewCmdDelete(cli, nil))
	cmd.AddCommand(listCmd.NewCmdList(cli, nil))
	cmd.AddCommand(setCmd.NewCmdSet(cli, nil))

	return cmd
}

package auth

import (
	authLoginCmd "github.com/kittycad/cli/cmd/auth/login"
	authLogoutCmd "github.com/kittycad/cli/cmd/auth/logout"
	authStatusCmd "github.com/kittycad/cli/cmd/auth/status"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/kittycad/cli/pkg/cmdutil"
	"github.com/spf13/cobra"
)

// NewCmdAuth returns a new instance of the auth command.
func NewCmdAuth(cli *cli.CLI) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "auth <command>",
		Short: "Login, logout, and get the status of your authentication",
		Long:  `Manage kittycad's authentication state.`,
	}

	cmdutil.DisableAuthCheck(cmd)

	cmd.AddCommand(authLoginCmd.NewCmdLogin(cli, nil))
	cmd.AddCommand(authLogoutCmd.NewCmdLogout(cli, nil))
	cmd.AddCommand(authStatusCmd.NewCmdStatus(cli, nil))

	return cmd
}

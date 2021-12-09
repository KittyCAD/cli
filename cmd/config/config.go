package config

import (
	"fmt"
	"strings"

	cmdGet "github.com/kittycad/cli/cmd/config/get"
	cmdList "github.com/kittycad/cli/cmd/config/list"
	cmdSet "github.com/kittycad/cli/cmd/config/set"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/kittycad/cli/pkg/cmdutil"
	"github.com/spf13/cobra"
)

// NewCmdConfig creates the config command.
func NewCmdConfig(cli *cli.CLI) *cobra.Command {
	longDoc := strings.Builder{}
	longDoc.WriteString("Display or change configuration settings for kittycad.\n\n")
	longDoc.WriteString("Current respected settings:\n")
	for _, co := range config.Options() {
		longDoc.WriteString(fmt.Sprintf("- %s: %s", co.Key, co.Description))
		if co.DefaultValue != "" {
			longDoc.WriteString(fmt.Sprintf(" (default: %q)", co.DefaultValue))
		}
		longDoc.WriteRune('\n')
	}

	cmd := &cobra.Command{
		Use:   "config <command>",
		Short: "Manage configuration for kittycad",
		Long:  longDoc.String(),
	}

	cmdutil.DisableAuthCheck(cmd)

	cmd.AddCommand(cmdGet.NewCmdConfigGet(cli, nil))
	cmd.AddCommand(cmdSet.NewCmdConfigSet(cli, nil))
	cmd.AddCommand(cmdList.NewCmdConfigList(cli, nil))

	return cmd
}

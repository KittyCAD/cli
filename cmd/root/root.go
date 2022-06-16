package root

import (
	"github.com/MakeNowJust/heredoc"
	aliasCmd "github.com/kittycad/cli/cmd/alias"
	apiCallCmd "github.com/kittycad/cli/cmd/api-call"
	authCmd "github.com/kittycad/cli/cmd/auth"
	completionCmd "github.com/kittycad/cli/cmd/completion"
	configCmd "github.com/kittycad/cli/cmd/config"
	drakeCmd "github.com/kittycad/cli/cmd/drake"
	fileCmd "github.com/kittycad/cli/cmd/file"
	openCmd "github.com/kittycad/cli/cmd/open"
	versionCmd "github.com/kittycad/cli/cmd/version"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/kittycad/cli/pkg/cmdutil"
	"github.com/kittycad/cli/version"
	"github.com/spf13/cobra"
)

// NewCmdRoot creates the root command and its nested children.
func NewCmdRoot(cli *cli.CLI) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "kittycad <command> <subcommand> [flags]",
		Short: "KittyCAD CLI",
		Long:  `You've never CAD it so good.`,

		SilenceErrors: true,
		SilenceUsage:  true,
		Example: heredoc.Doc(`
			# convert a step file to a dxf file
			$ kittycad file convert ./button.step --to=dxf

			# get the status of an asynchronous file conversion
			$ kittycad file status <uuid_of_conversion>
		`),
		Annotations: map[string]string{
			"help:feedback": heredoc.Doc(`
				Open an issue on https://github.com/kittycad/cli/issues, or run ` + "`kittycad open issue`" + `
			`),
			"help:environment": heredoc.Doc(`
				See 'kittycad help environment' for the list of supported environment variables.
			`),
		},
	}

	cmd.SetOut(cli.IOStreams.Out)
	cmd.SetErr(cli.IOStreams.ErrOut)

	cmd.PersistentFlags().Bool("help", false, "Show help for command")
	cmd.SetHelpFunc(func(cmd *cobra.Command, args []string) {
		rootHelpFunc(cli, cmd, args)
	})
	cmd.SetUsageFunc(rootUsageFunc)
	cmd.SetFlagErrorFunc(rootFlagErrorFunc)

	formattedVersion := versionCmd.Format(version.VERSION, version.GITCOMMIT)
	cmd.SetVersionTemplate(formattedVersion)
	cmd.Version = formattedVersion
	cmd.Flags().Bool("version", false, "Show kittycad version")

	// Child commands
	cmd.AddCommand(versionCmd.NewCmdVersion(cli))
	cmd.AddCommand(aliasCmd.NewCmdAlias(cli))
	cmd.AddCommand(apiCallCmd.NewCmdAPICall(cli))
	cmd.AddCommand(authCmd.NewCmdAuth(cli))
	cmd.AddCommand(configCmd.NewCmdConfig(cli))
	cmd.AddCommand(completionCmd.NewCmdCompletion(cli.IOStreams))
	cmd.AddCommand(drakeCmd.NewCmdDrake(cli, nil))
	cmd.AddCommand(fileCmd.NewCmdFile(cli))
	cmd.AddCommand(openCmd.NewCmdOpen(cli, nil))

	// Help topics
	cmd.AddCommand(NewHelpTopic("environment"))
	cmd.AddCommand(NewHelpTopic("formatting"))
	cmd.AddCommand(NewHelpTopic("mintty"))
	referenceCmd := NewHelpTopic("reference")
	referenceCmd.SetHelpFunc(referenceHelpFn(cli.IOStreams))
	cmd.AddCommand(referenceCmd)

	cmdutil.DisableAuthCheck(cmd)

	// this needs to appear last:
	referenceCmd.Long = referenceLong(cmd)
	return cmd
}

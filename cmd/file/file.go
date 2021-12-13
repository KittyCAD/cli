package file

import (
	"github.com/MakeNowJust/heredoc"
	cmdConvert "github.com/kittycad/cli/cmd/file/convert"
	cmdStatus "github.com/kittycad/cli/cmd/file/status"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

// NewCmdFile returns a new instance of the file command.
func NewCmdFile(cli *cli.CLI) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "file <command>",
		Short: "CAD file operations",
		Long:  `Perform operations like conversions on CAD files`,
		Example: heredoc.Doc(`
			# convert a step file to an obj file
			$ kittycad file convert ./input.step --to=obj

			# get the status of an asynchronous file conversion
			$ kittycad file status <uuid_of_conversion>
		`),
		Annotations: map[string]string{
			"IsCore": "true",
		},
	}

	cmd.AddCommand(cmdConvert.NewCmdConvert(cli, nil))
	cmd.AddCommand(cmdStatus.NewCmdStatus(cli, nil))

	return cmd
}

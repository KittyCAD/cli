package file

import (
	"github.com/MakeNowJust/heredoc"
	cmdConvert "github.com/kittycad/cli/cmd/file/convert"
	cmdStatus "github.com/kittycad/cli/cmd/file/status"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

func NewCmdFile(cli *cli.CLI) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "file <command>",
		Short: "CAD file operations",
		Long:  `Perform operations like conversions on CAD files`,
		Example: heredoc.Doc(`
			$ kittycad file convert ./input.dxf --to=dwg
			$ kittycad file status <uuid_of_conversion>
		`),
		Annotations: map[string]string{
			"IsCore": "true",
		},
	}

	cmd.AddCommand(cmdConvert.NewCmdConvert(cli))
	cmd.AddCommand(cmdStatus.NewCmdStatus(cli))

	return cmd
}

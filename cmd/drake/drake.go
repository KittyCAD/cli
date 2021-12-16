package drake

import (
	"fmt"

	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

type browser interface {
	Browse(string) error
}

// Options are the options for drake.
type Options struct {
	Browser browser
	IO      *iostreams.IOStreams
}

const drakeMemeURL string = "https://dl.kittycad.com/drake.jpeg"

// NewCmdDrake creates a new `drake` command.
func NewCmdDrake(cli *cli.CLI, runF func(*Options) error) *cobra.Command {
	opts := &Options{
		Browser: cli.Browser,
		IO:      cli.IOStreams,
	}

	cmd := &cobra.Command{
		Long:  "Open a drake meme in your web browser.",
		Short: "Best I ever CAD",
		Use:   "drake",
		Example: heredoc.Doc(`
			$ kittycad drake
		`),
		Annotations: map[string]string{},
		RunE: func(cmd *cobra.Command, args []string) error {
			return runDrake(opts)
		},
	}

	return cmd
}

func runDrake(opts *Options) error {
	if opts.IO.IsStdoutTTY() {
		fmt.Fprintf(opts.IO.Out, "Opening %s in your browser.\n", drakeMemeURL)
	}

	return opts.Browser.Browse(drakeMemeURL)
}

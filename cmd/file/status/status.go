package status

import (
	"context"
	"fmt"
	"io/ioutil"
	"time"

	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/kittycad/cli/cmd/file/shared"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/kittycad/kittycad.go"
	"github.com/spf13/cobra"
)

// Options defines the options of the `file stattus` command.
type Options struct {
	IO             *iostreams.IOStreams
	KittyCADClient func(string) (*kittycad.Client, error)
	Context        context.Context

	ID string

	// Flag options.
	OutputFile string
}

// NewCmdStatus returns a new instance of the status command.
func NewCmdStatus(cli *cli.CLI, runF func(*Options) error) *cobra.Command {
	opts := &Options{
		IO:             cli.IOStreams,
		KittyCADClient: cli.KittyCADClient,
		Context:        cli.Context,
	}

	cmd := &cobra.Command{
		Use:   "status <id>",
		Short: "Get a file conversion",
		Long: heredoc.Docf(`
			Get the status of a file conversion.

			This only works for file conversions that are being performed
			asynchronously.
		`),
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) > 0 {
				opts.ID = args[0]
			}

			if runF != nil {
				return runF(opts)
			}

			return statusRun(opts)
		},
	}

	cmd.Flags().StringVarP(&opts.OutputFile, "output", "o", "", "The output file path to save the contents to.")

	return cmd
}

func statusRun(opts *Options) error {
	kittycadClient, err := opts.KittyCADClient("")
	if err != nil {
		return err
	}

	// Do the conversion.
	conversion, output, err := kittycadClient.File.ConversionByIDWithBase64Helper(opts.ID)
	if err != nil {
		return fmt.Errorf("error getting file conversion %s: %w", opts.ID, err)
	}

	// If they specified an output file, write the output to it.
	if len(output) > 0 && opts.OutputFile != "" {
		if err := ioutil.WriteFile(opts.OutputFile, output, 0644); err != nil {
			return fmt.Errorf("error writing output to file `%s`: %w", opts.OutputFile, err)
		}
	}

	// Let's get the duration.
	completedAt := time.Now()
	if conversion.CompletedAt != nil {
		completedAt = *conversion.CompletedAt.Time
	}
	duration := completedAt.Sub(*conversion.CreatedAt.Time)

	connectedToTerminal := opts.IO.IsStdoutTTY() && opts.IO.IsStderrTTY()

	opts.IO.DetectTerminalTheme()

	err = opts.IO.StartPager()
	if err != nil {
		return err
	}
	defer opts.IO.StopPager()

	if connectedToTerminal {
		return shared.PrintHumanConversion(opts.IO, conversion, output, opts.OutputFile, duration)
	}

	return shared.PrintRawConversion(opts.IO, conversion, output, opts.OutputFile, duration)
}

package status

import (
	"context"
	"fmt"
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
		Short: "Get an async API call",
		Long: heredoc.Docf(`
			Get the status of an asynchronous API call.

			This only works for API calls that are being performed
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

	return cmd
}

func statusRun(opts *Options) error {
	kittycadClient, err := opts.KittyCADClient("")
	if err != nil {
		return err
	}

	// Do the conversion.
	asyncAPICall, err := kittycadClient.APICall.GetAsyncOperation(opts.ID)
	if err != nil {
		return fmt.Errorf("error getting async operation %s: %w", opts.ID, err)
	}

	// Let's get the duration.
	completedAt := time.Now()
	if asyncAPICall.CompletedAt != nil && asyncAPICall.CompletedAt.Time != nil {
		completedAt = *asyncAPICall.CompletedAt.Time
	}
	duration := completedAt.Sub(*asyncAPICall.CreatedAt.Time)

	connectedToTerminal := opts.IO.IsStdoutTTY() && opts.IO.IsStderrTTY()

	opts.IO.DetectTerminalTheme()

	err = opts.IO.StartPager()
	if err != nil {
		return err
	}
	defer opts.IO.StopPager()

	if connectedToTerminal {
		return shared.PrintHumanAsyncAPICallOutput(opts.IO, asyncAPICall, []byte{}, "", duration)
	}

	return shared.PrintRawAsyncAPICall(opts.IO, asyncAPICall, []byte{}, "", duration)
}

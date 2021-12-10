package instance

import (
	"context"
	"fmt"

	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/kittycad/cli/kittycad"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/kittycad/cli/pkg/cmdutil"
	"github.com/spf13/cobra"
)

// Options for the instance command.
type Options struct {
	KittyCADClient func() (*kittycad.Client, error)
	IO             *iostreams.IOStreams
	Exporter       cmdutil.Exporter
	Context        context.Context
}

// TODO: Have the spec generate the fields as well as the client.
var fields = []string{
	"id",
	"git_hash",
	"environment",
	"name",
	"description",
	"ip_address",
	"zone",
	"image",
	"hostname",
	"cpu_platform",
	"machine_type",
}

// NewCmdInstance returns a new instance command.
func NewCmdInstance(cli *cli.CLI) *cobra.Command {
	opts := &Options{
		IO:             cli.IOStreams,
		KittyCADClient: cli.KittyCADClient,
		Context:        cli.Context,
	}

	cmd := &cobra.Command{
		Use:   "instance",
		Short: "Get instance metadata",
		Long: heredoc.Doc(`
				Get information about this specific API server instance.
				This is primarily used for debugging
				`),

		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			return instanceRun(opts)
		},
	}

	// TODO: Actually get the JSON flags to work.
	cmdutil.AddJSONFlags(cmd, &opts.Exporter, fields)

	return cmd
}

func instanceRun(opts *Options) error {
	kittycadClient, err := opts.KittyCADClient()
	if err != nil {
		return err
	}

	// Get the instance.
	instance, err := kittycadClient.MetaDebugInstance(opts.Context)
	if err != nil {
		return fmt.Errorf("failed to get auth server instance: %w", err)
	}

	connectedToTerminal := opts.IO.IsStdoutTTY() && opts.IO.IsStderrTTY()

	opts.IO.DetectTerminalTheme()

	err = opts.IO.StartPager()
	if err != nil {
		return err
	}
	defer opts.IO.StopPager()

	if opts.Exporter != nil {
		return opts.Exporter.Write(opts.IO, instance)
	}

	if connectedToTerminal {
		return printHumanInstance(opts, instance)
	}

	return printRawInstance(opts.IO, instance)
}

func printRawInstance(io *iostreams.IOStreams, instance *kittycad.InstanceMetadata) error {
	out := io.Out
	cs := io.ColorScheme()

	fmt.Fprintf(out, "id:\t\t%s\n", *instance.Id)
	fmt.Fprintf(out, "git hash:\t%s\n", *instance.GitHash)
	fmt.Fprintf(out, "environment:\t%s\n", formattedEnvironment(cs, *instance.Environment))
	fmt.Fprintf(out, "name:\t\t%s\n", *instance.Name)
	if *instance.Description != "" {
		fmt.Fprintf(out, "description:\t%s\n", *instance.Description)
	}
	fmt.Fprintf(out, "ip address:\t%s\n", *instance.IpAddress)
	fmt.Fprintf(out, "zone:\t\t%s\n", *instance.Zone)
	fmt.Fprintf(out, "image:\t\t%s\n", *instance.Image)
	fmt.Fprintf(out, "hostname:\t%s\n", *instance.Hostname)
	fmt.Fprintf(out, "cpu platform:\t%s\n", *instance.CpuPlatform)
	fmt.Fprintf(out, "machine type:\t%s\n", *instance.MachineType)

	return nil
}

func printHumanInstance(opts *Options, instance *kittycad.InstanceMetadata) error {
	out := opts.IO.Out
	cs := opts.IO.ColorScheme()

	// Name (GitHash and Environment)
	fmt.Fprintf(out, "%s (%s %s)\n", cs.Bold(*instance.Name), *instance.GitHash, formattedEnvironment(cs, *instance.Environment))
	if *instance.Description != "" {
		fmt.Fprintf(out, "%s\n", *instance.Description)
	}
	fmt.Fprintf(out, "\nip address:\t%s\n", *instance.IpAddress)
	fmt.Fprintf(out, "zone:\t\t%s\n", *instance.Zone)
	fmt.Fprintf(out, "image:\t\t%s\n", *instance.Image)
	fmt.Fprintf(out, "hostname:\t%s\n", *instance.Hostname)
	fmt.Fprintf(out, "cpu platform:\t%s\n", *instance.CpuPlatform)
	fmt.Fprintf(out, "machine type:\t%s\n", *instance.MachineType)

	return nil
}

// formattedEnvironment formats an environment with state color.
func formattedEnvironment(cs *iostreams.ColorScheme, environment kittycad.Environment) string {
	var colorFunc func(string) string
	switch environment {
	case kittycad.EnvironmentDEVELOPMENT:
		colorFunc = cs.Yellow
	case kittycad.EnvironmentPREVIEW:
		colorFunc = cs.Blue
	case kittycad.EnvironmentPRODUCTION:
		colorFunc = cs.Green
	default:
		colorFunc = func(str string) string { return str } // Do nothing
	}

	return colorFunc(string(environment))
}

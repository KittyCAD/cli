package main

import (
	"context"
	"flag"

	"github.com/kittycad/cli/version"

	"github.com/genuinetools/pkg/cli"
)

func main() {
	// Create a new cli program.
	p := cli.NewProgram()
	p.Name = "kittycad"
	p.Description = "The KittyCAD command line tool"

	// Set the GitCommit and Version.
	p.GitCommit = version.GITCOMMIT
	p.Version = version.VERSION

	// Build the list of available commands.
	p.Commands = []cli.Command{}

	// Setup the global flags.
	p.FlagSet = flag.NewFlagSet("global", flag.ExitOnError)

	// Set the before function.
	p.Before = func(ctx context.Context) error {
		return nil
	}

	// Set the main program action.
	p.Action = func(ctx context.Context, args []string) error {
		return nil
	}

	// Run our program.
	p.Run()
}

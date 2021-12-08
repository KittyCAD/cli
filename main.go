package main

import (
	"fmt"
	"log"
	"os"

	"github.com/kittycad/cli/version"

	"github.com/kittycad/kittycad.go"
	"github.com/urfave/cli/v2"
)

var kittycadClient *kittycad.Client

func main() {

	// Create a new cli program.
	app := &cli.App{
		Name:  "kittycad",
		Usage: "Work seamlessly with KittyCAD from the command line",
		// Set the GitCommit and Version.
		Version: fmt.Sprintf("%s (%s)",
			version.VERSION,
			version.GITCOMMIT),
		EnableBashCompletion: true,
		Before: func(c *cli.Context) error {
			// See if we are authorized to use the API.
			// TODO: Optionally have an auth command and set the token in a file.
			var err error
			kittycadClient, err = kittycad.NewDefaultClientFromEnv()
			if err != nil {
				return err
			}
			return nil
		},
		// TODO: Generate the subcommand boilerplate. This way it gets the docs from the spec.
		Commands: []*cli.Command{
			// TODO: Add a command like github's `api` command.
			{
				Name:  "file",
				Usage: "CAD file operations.",
				Subcommands: []*cli.Command{
					{
						Name:  "convert",
						Usage: "Convert a CAD file from one format to another. If the file being converted is larger than a certain size it will be performed asynchronously.",
						Action: func(c *cli.Context) error {
							fmt.Println("new task template: ", c.Args().First())
							return nil
						},
					},
					{
						Name:  "status",
						Usage: "Get the status of a file conversion.",
						Action: func(c *cli.Context) error {
							fmt.Println("removed task template: ", c.Args().First())
							return nil
						},
					},
				},
			},
			{
				Name:  "meta",
				Usage: "Meta information about servers and instances.",
				Subcommands: []*cli.Command{
					{
						Name:  "session",
						Usage: "Get information about your API request session. This is primarily used for debugging.",
						Action: func(c *cli.Context) error {
							return metaSession(c)
						},
					},
					{
						Name:  "instance",
						Usage: "Get information about this specific API server instance. This is primarily used for debugging.",
						Action: func(c *cli.Context) error {
							return metaInstance(c)
						},
					},
				},
			},
		},
	}

	err := app.Run(os.Args)
	if err != nil {
		log.Fatal(err)
	}
}

package open

import (
	"fmt"
	"sort"
	"strings"

	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

type browser interface {
	Browse(string) error
}

// Options are the options for the open command.
type Options struct {
	Browser browser
	IO      *iostreams.IOStreams

	SelectedSite string
	SelectedURL  string
}

var links = map[string]string{
	"account":    "https://kittycad.io/account",
	"blog":       "https://kittycad.io/blog",
	"discord":    "https://discord.com/invite/Bee65eqawJ",
	"issue":      "https://github.com/KittyCAD/cli/issues",
	"discussion": "https://github.com/KittyCAD/cli/discussions",
	"docs":       "https://docs.kittycad.io",
	"github":     "https://github.com/kittycad/cli",
	"store":      "https://store.kittycad.io",
}

// NewCmdOpen creates a new `open` command.
func NewCmdOpen(cli *cli.CLI, runF func(*Options) error) *cobra.Command {
	opts := &Options{
		Browser: cli.Browser,
		IO:      cli.IOStreams,
	}

	// Get the keys of the map.
	keys := make([]string, len(links))
	i := 0
	for k := range links {
		keys[i] = k
		i++
	}
	// Sort the keys.
	sort.Strings(keys)

	cmd := &cobra.Command{
		Short: "Open a KittyCAD site",
		Long:  "Shortcut to open KittyCAD sites in your browser.",
		Use:   fmt.Sprintf("open {%s}", strings.Join(keys, " | ")),
		Example: heredoc.Doc(`
			# open the KittyCAD docs in your browser
			$ kittycad open docs

			# open your KittyCAD account in your browser
			$ kittycad open account
		`),
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) > 0 {
				opts.SelectedSite = strings.ToLower(args[0])
			}

			// Check if the selected site is valid.
			uri, ok := links[opts.SelectedSite]
			if !ok {
				return fmt.Errorf("invalid site: %s -- must be one of {%s}", opts.SelectedSite, strings.Join(keys, " | "))
			}
			opts.SelectedURL = uri

			if runF != nil {
				return runF(opts)
			}

			return runOpen(opts)
		},
	}

	return cmd
}

func runOpen(opts *Options) error {
	if opts.IO.IsStdoutTTY() {
		fmt.Fprintf(opts.IO.Out, "Opening %s in your browser.\n", opts.SelectedURL)
	}

	return opts.Browser.Browse(opts.SelectedURL)
}

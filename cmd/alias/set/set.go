package set

import (
	"fmt"
	"io/ioutil"
	"strings"

	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/google/shlex"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
)

// Options are the options for the `kittycad alias set` command.
type Options struct {
	Config func() (config.Config, error)
	IO     *iostreams.IOStreams

	Name      string
	Expansion string
	IsShell   bool

	validCommand func(string) bool
}

// NewCmdSet creates a new `kittycad alias set` command.
func NewCmdSet(cli *cli.CLI, runF func(*Options) error) *cobra.Command {
	opts := &Options{
		IO:     cli.IOStreams,
		Config: cli.Config,
	}

	cmd := &cobra.Command{
		Use:   "set <alias> <expansion>",
		Short: "Create a shortcut for a kittycad command",
		Long: heredoc.Doc(`
			Define a word that will expand to a full kittycad command when invoked.

			The expansion may specify additional arguments and flags. If the expansion includes
			positional placeholders such as "$1", extra arguments that follow the alias will be
			inserted appropriately. Otherwise, extra arguments will be appended to the expanded
			command.

			Use "-" as expansion argument to read the expansion string from standard input. This
			is useful to avoid quoting issues when defining expansions.

			If the expansion starts with "!" or if "--shell" was given, the expansion is a shell
			expression that will be evaluated through the "sh" interpreter when the alias is
			invoked. This allows for chaining multiple commands via piping and redirection.
		`),
		Example: heredoc.Doc(`
			# note: Command Prompt on Windows requires using double quotes for arguments
			$ kittycad alias set fc 'file convert'
			$ kittycad fc ./thing.obj --to step  #=> kittycad file convert ./thing.obj --to step
		`),
		Args: cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			opts.Name = args[0]
			opts.Expansion = args[1]

			opts.validCommand = func(args string) bool {
				split, err := shlex.Split(args)
				if err != nil {
					return false
				}

				rootCmd := cmd.Root()
				cmd, _, err := rootCmd.Traverse(split)
				if err == nil && cmd != rootCmd {
					return true
				}

				return false
			}

			if runF != nil {
				return runF(opts)
			}
			return setRun(opts)
		},
	}

	cmd.Flags().BoolVarP(&opts.IsShell, "shell", "s", false, "Declare an alias to be passed through a shell interpreter")

	return cmd
}

func setRun(opts *Options) error {
	cs := opts.IO.ColorScheme()
	cfg, err := opts.Config()
	if err != nil {
		return err
	}

	aliasCfg, err := cfg.Aliases()
	if err != nil {
		return err
	}

	expansion, err := getExpansion(opts)
	if err != nil {
		return fmt.Errorf("did not understand expansion: %w", err)
	}

	isTerminal := opts.IO.IsStdoutTTY()
	if isTerminal {
		fmt.Fprintf(opts.IO.ErrOut, "- Adding alias for %s: %s\n", cs.Bold(opts.Name), cs.Bold(expansion))
	}

	isShell := opts.IsShell
	if isShell && !strings.HasPrefix(expansion, "!") {
		expansion = "!" + expansion
	}
	isShell = strings.HasPrefix(expansion, "!")

	if opts.validCommand(opts.Name) {
		return fmt.Errorf("could not create alias: %q is already a kittycad command", opts.Name)
	}

	if !isShell && !opts.validCommand(expansion) {
		return fmt.Errorf("could not create alias: %s does not correspond to a kittycad command", expansion)
	}

	successMsg := fmt.Sprintf("%s Added alias.", cs.SuccessIcon())
	if oldExpansion, ok := aliasCfg.Get(opts.Name); ok {
		successMsg = fmt.Sprintf("%s Changed alias %s from %s to %s",
			cs.SuccessIcon(),
			cs.Bold(opts.Name),
			cs.Bold(oldExpansion),
			cs.Bold(expansion),
		)
	}

	err = aliasCfg.Add(opts.Name, expansion)
	if err != nil {
		return fmt.Errorf("could not create alias: %s", err)
	}

	if isTerminal {
		fmt.Fprintln(opts.IO.ErrOut, successMsg)
	}

	return nil
}

func getExpansion(opts *Options) (string, error) {
	if opts.Expansion == "-" {
		stdin, err := ioutil.ReadAll(opts.IO.In)
		if err != nil {
			return "", fmt.Errorf("failed to read from STDIN: %w", err)
		}

		return string(stdin), nil
	}

	return opts.Expansion, nil
}

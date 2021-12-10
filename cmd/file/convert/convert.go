package convert

import (
	"context"
	"errors"
	"fmt"
	"path/filepath"
	"strings"

	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/kittycad/cli/kittycad"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/kittycad/cli/pkg/cmdutil"
	"github.com/spf13/cobra"
)

// Options defines the options of the `file convert` command.
type Options struct {
	IO             *iostreams.IOStreams
	KittyCADClient func() (*kittycad.Client, error)
	Context        context.Context

	// Flag options.
	InputFileArg  string
	InputFormat   string
	InputFileBody []byte
	OutputFormat  string
}

// TODO: use an enum for these and generate them in the go api library.
const (
	formatSTEP = "step"
	formatOBJ  = "obj"
	formatDXF  = "dxf"
)

var validFormats = []string{formatSTEP, formatOBJ, formatDXF}

// NewCmdConvert creates a new cobra.Command for the convert subcommand.
func NewCmdConvert(cli *cli.CLI, runF func(*Options) error) *cobra.Command {
	opts := &Options{
		IO:             cli.IOStreams,
		KittyCADClient: cli.KittyCADClient,
		Context:        cli.Context,
	}

	cmd := &cobra.Command{
		Use:   "convert <path>",
		Short: "Convert CAD file",
		Long: heredoc.Docf(`
			Convert a CAD file from one format to another.

			If the file being converted is larger than a certain size it will be
			performed asynchronously, you can then check its status with the
			%[1]sfile status%[1]s command.
		`, "`"),
		Example: heredoc.Doc(`
			# convert obj to step
			$ kittycad file convert  my-obj.obj --output-format step

			# convert step to obj
			$ kittycad file convert	 my-step.step -o obj

			# pass a file to convert from stdin
			# when converting from stdin, the original file type is required
			$ cat my-obj.obj | kittycad file convert - --output-format step --from obj
		`),
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) > 0 {
				opts.InputFileArg = args[0]
			}

			b, err := cmdutil.ReadFile(opts.InputFileArg, opts.IO.In)
			if err != nil {
				return err
			}
			opts.InputFileBody = b

			// Get the file extension type for the file.
			ext := strings.TrimPrefix(strings.ToLower(filepath.Ext(opts.InputFileArg)), ".")
			if ext == "" && opts.InputFormat == "" {
				return errors.New("input file must have an extension or you mut pass the file type with `--from` or `-i`")
			}
			// Standardize the input format to lowercase.
			opts.InputFormat = strings.ToLower(opts.InputFormat)
			// Ensure the two types match.
			if ext != "" && opts.InputFormat != "" && ext != opts.InputFormat {
				return fmt.Errorf("input file extension and file type must match, got extension `%s` and input format `%s`", ext, opts.InputFormat)
			}

			// Set the extension to the input format if it was not set.
			if opts.InputFormat == "" {
				opts.InputFormat = ext
			}

			// Validate the extension is a supported file format.
			if !contains(validFormats, opts.InputFormat) {
				return fmt.Errorf("unsupported input file format: `%s`", ext)
			}

			// Validate the output format is a supported file format.
			if !contains(validFormats, opts.OutputFormat) {
				return fmt.Errorf("unsupported output file format: `%s`", opts.OutputFormat)
			}

			if opts.InputFormat == opts.OutputFormat {
				return fmt.Errorf("input and output file formats must be different, both are: `%s`", opts.InputFormat)
			}

			if runF != nil {
				return runF(opts)
			}

			// Now we can continue with the conversion.
			return convertRun(opts)
		},
	}

	cmd.Flags().StringVarP(&opts.OutputFormat, "output-format", "o", "", "The output format to convert to.")
	cmd.Flags().StringVarP(&opts.InputFormat, "from", "i", "", "The input format we are converting from (required when the input file is from stdin or lacks a file extension).")

	return cmd
}

func convertRun(opts *Options) error {
	kittycadClient, err := opts.KittyCADClient()
	if err != nil {
		return err
	}

	// Do the conversion.
	conversion, err := kittycadClient.FileConvert(opts.Context, opts.InputFormat, opts.OutputFormat, opts.InputFileBody)
	if err != nil {
		return fmt.Errorf("error converting file: %w", err)
	}

	// Print the output of the conversion.
	fmt.Fprintf(opts.IO.Out, "%#v", conversion)

	return nil
}

func contains(s []string, str string) bool {
	for _, v := range s {
		if v == str {
			return true
		}
	}

	return false
}

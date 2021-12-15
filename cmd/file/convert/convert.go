package convert

import (
	"bytes"
	"context"
	"encoding/base64"
	"errors"
	"fmt"
	"io"
	"io/ioutil"
	"path/filepath"
	"strings"
	"time"

	"github.com/MakeNowJust/heredoc"
	"github.com/cheggaaa/pb/v3"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/kittycad/cli/cmd/file/shared"
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
	OutputFile    string
}

// NewCmdConvert creates a new cobra.Command for the convert subcommand.
func NewCmdConvert(cli *cli.CLI, runF func(*Options) error) *cobra.Command {
	opts := &Options{
		IO:             cli.IOStreams,
		KittyCADClient: cli.KittyCADClient,
		Context:        cli.Context,
	}

	validFormats := []string{}
	for _, v := range kittycad.ValidFileTypes {
		validFormats = append(validFormats, string(v))
	}

	cmd := &cobra.Command{
		Use:   "convert <source-filepath> [<output-filepath>]",
		Short: "Convert CAD file",
		Long: heredoc.Docf(`
			Convert a CAD file from one format to another.

			If the file being converted is larger than a certain size it will be
			performed asynchronously, you can then check its status with the
			%[1]skittycad file status%[1]s command.

			Valid formats: %[2]s
		`, "`", strings.Join(validFormats, ", ")),
		Example: heredoc.Doc(`
			# convert step to obj and save to file
			$ kittycad file convert my-file.step my-file.obj

			# convert obj to step and print to stdout
			$ kittycad file convert my-obj.obj --to step

			# convert step to obj and print to stdout
			$ kittycad file convert my-step.step -t obj

			# pass a file to convert from stdin and print to stdout
			# when converting from stdin, the original file type is required
			$ cat my-obj.obj | kittycad file convert - --to step --from obj
		`),
		Args: cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) > 0 {
				opts.InputFileArg = args[0]
			}

			if len(args) > 1 {
				opts.OutputFile = args[1]
			}

			// Get the file extension type for the input file.
			ext := getExtension(opts.InputFileArg)
			if ext == "" && opts.InputFormat == "" {
				return errors.New("input file must have an extension or you must pass the file type with `--from` or `-f`")
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

			if opts.OutputFile != "" {
				// Get the file extension type for the output file.
				ext = getExtension(opts.OutputFile)
				if ext == "" && opts.OutputFormat == "" {
					return errors.New("output file must have an extension or you must pass the file type with `--to` or `-t`")
				}
				// Standardize the output format to lowercase.
				opts.OutputFormat = strings.ToLower(opts.OutputFormat)
				// Ensure the two types match.
				if ext != "" && opts.OutputFormat != "" && ext != opts.OutputFormat {
					return fmt.Errorf("output file extension and file type must match, got extension `%s` and output format `%s`", ext, opts.OutputFormat)
				}
				// Set the extension to the output format if it was not set.
				if opts.OutputFormat == "" {
					opts.OutputFormat = ext
				}
			}

			// Validate the output format is a supported file format.
			if !contains(validFormats, opts.OutputFormat) {
				return fmt.Errorf("unsupported output file format: `%s`", opts.OutputFormat)
			}

			if opts.InputFormat == opts.OutputFormat {
				return fmt.Errorf("input and output file formats must be different, both are: `%s`", opts.InputFormat)
			}

			b, err := cmdutil.ReadFile(opts.InputFileArg, opts.IO.In)
			if err != nil {
				return err
			}
			opts.InputFileBody = b

			if runF != nil {
				return runF(opts)
			}

			// Now we can continue with the conversion.
			return convertRun(opts)
		},
	}

	cmd.Flags().StringVarP(&opts.OutputFormat, "to", "t", "", "The output format to convert to.")
	cmd.Flags().StringVarP(&opts.InputFormat, "from", "f", "", "The input format we are converting from (required when the input file is from stdin or lacks a file extension).")

	return cmd
}

func convertRun(opts *Options) error {
	kittycadClient, err := opts.KittyCADClient()
	if err != nil {
		return err
	}

	// Do the conversion.
	conversion, output, err := doConversion(kittycadClient, kittycad.ValidFileType(opts.InputFormat), kittycad.ValidFileType(opts.OutputFormat), opts.InputFileBody, opts)
	if err != nil {
		return fmt.Errorf("error converting file: %w", err)
	}

	// If they specified an output file, write the output to it.
	if len(output) > 0 && opts.OutputFile != "" {
		if err := ioutil.WriteFile(opts.OutputFile, output, 0644); err != nil {
			return fmt.Errorf("error writing output to file `%s`: %w", opts.OutputFile, err)
		}
	}

	fmt.Printf("%#v", conversion)
	// Let's get the duration.
	completedAt := time.Now()
	if conversion.CompletedAt != nil {
		completedAt = *conversion.CompletedAt
	}
	duration := completedAt.Sub(*conversion.CreatedAt)

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

func contains(s []string, str string) bool {
	for _, v := range s {
		if v == str {
			return true
		}
	}

	return false
}

func getExtension(file string) string {
	return strings.TrimPrefix(strings.ToLower(filepath.Ext(file)), ".")
}

func doConversion(c *kittycad.Client, srcFormat kittycad.ValidFileType, outputFormat kittycad.ValidFileType, body []byte, opts *Options) (*kittycad.FileConversion, []byte, error) {
	var b bytes.Buffer
	encoder := base64.NewEncoder(base64.StdEncoding, &b)
	// Encode the body as base64.
	encoder.Write(body)
	// Must close the encoder when finished to flush any partial blocks.
	// If you comment out the following line, the last partial block "r"
	// won't be encoded.
	encoder.Close()

	connectedToTerminal := opts.IO.IsStdoutTTY() && opts.IO.IsStderrTTY()

	// Initialize the progress bar.
	var bodyReader io.Reader
	bodyReader = &b
	var bar *pb.ProgressBar

	fmt.Println("Sending conversion request...")
	if connectedToTerminal {
		// Create a new progress bar.
		bar = pb.New(b.Len()).Set(pb.Bytes, true).SetRefreshRate(time.Millisecond * 10)

		// Start the progress bar.
		bar.Start()
		bodyReader = bar.NewProxyReader(&b)
	}

	// TODO: Make it so the progress bar does not update until we get a response.
	resp, err := c.FileConvert(srcFormat, outputFormat, bodyReader)
	if err != nil {
		return nil, nil, err
	}

	if connectedToTerminal {
		// Stop the progress bar if we were using one.
		bar.Finish()
	}

	if resp.Output == "" {
		return resp, nil, nil
	}

	// Decode the base64 encoded body.
	output, err := base64.StdEncoding.DecodeString(resp.Output)
	if err != nil {
		return nil, nil, fmt.Errorf("base64 decoding output from API failed: %v", err)
	}

	return resp, output, nil
}

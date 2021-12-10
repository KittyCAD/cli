package convert

import (
	"bytes"
	"fmt"
	"io/ioutil"
	"log"
	"os"
	"testing"

	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/google/shlex"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewCmdConvert(t *testing.T) {
	// Create a temporary file for testing.
	file, err := ioutil.TempFile("", "example.*.stl")
	if err != nil {
		log.Fatal(err)
	}
	defer os.Remove(file.Name())

	tests := []struct {
		name       string
		cli        string
		isTTY      bool
		wants      Options
		wantStdout string
		wantStderr string
		wantErr    string
	}{
		{
			name:       "file does not exist",
			cli:        "./does-not-exist.stl other-thing.obj",
			isTTY:      true,
			wants:      Options{},
			wantStdout: "",
			wantStderr: "",
			wantErr:    "file does not exist: ./does-not-exist.stl",
		},
		{
			name:       "conversion to identical format",
			cli:        fmt.Sprintf("%s other-thing.stl", file.Name()),
			isTTY:      true,
			wants:      Options{},
			wantStdout: "",
			wantStderr: "",
			wantErr:    "input and output file formats must be different, both are: `stl`",
		},
		{
			name:       "conversion to identical format with flag",
			cli:        fmt.Sprintf("%s -t stl", file.Name()),
			isTTY:      true,
			wants:      Options{},
			wantStdout: "",
			wantStderr: "",
			wantErr:    "input and output file formats must be different, both are: `stl`",
		},
		{
			name:       "input flag differs from extension",
			cli:        fmt.Sprintf("%s -t stl -f obj", file.Name()),
			isTTY:      true,
			wants:      Options{},
			wantStdout: "",
			wantStderr: "",
			wantErr:    "input file extension and file type must match, got extension `stl` and input format `obj`",
		},
		{
			name:       "output flag differs from extension",
			cli:        fmt.Sprintf("%s thing.obj -t step", file.Name()),
			isTTY:      true,
			wants:      Options{},
			wantStdout: "",
			wantStderr: "",
			wantErr:    "output file extension and file type must match, got extension `obj` and output format `step`",
		},
		{
			name:  "use file extension",
			cli:   fmt.Sprintf("%s thing.obj", file.Name()),
			isTTY: true,
			wants: Options{
				InputFileArg: file.Name(),
				InputFormat:  "stl",
				OutputFile:   "thing.obj",
				OutputFormat: "obj",
			},
			wantStdout: "",
			wantStderr: "",
			wantErr:    "",
		},
		{
			name:  "use output flag",
			cli:   fmt.Sprintf("%s -t obj", file.Name()),
			isTTY: true,
			wants: Options{
				InputFileArg: file.Name(),
				InputFormat:  "stl",
				OutputFile:   "",
				OutputFormat: "obj",
			},
			wantStdout: "",
			wantStderr: "",
			wantErr:    "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			io, _, stdout, stderr := iostreams.Test()
			io.SetStdoutTTY(tt.isTTY)
			io.SetStdinTTY(tt.isTTY)
			io.SetStderrTTY(tt.isTTY)

			c := &cli.CLI{
				IOStreams: io,
			}

			argv, err := shlex.Split(tt.cli)
			assert.NoError(t, err)
			var gotOpts *Options
			cmd := NewCmdConvert(c, func(opts *Options) error {
				gotOpts = opts
				return nil
			})

			cmd.SetArgs(argv)
			cmd.SetIn(&bytes.Buffer{})
			cmd.SetOut(&bytes.Buffer{})
			cmd.SetErr(&bytes.Buffer{})

			_, err = cmd.ExecuteC()
			if tt.wantErr != "" {
				assert.EqualError(t, err, tt.wantErr)
				return
			}
			require.NoError(t, err)

			assert.Equal(t, tt.wantStdout, stdout.String())
			assert.Equal(t, tt.wantStderr, stderr.String())
			assert.NoError(t, err)

			assert.Equal(t, tt.wants.InputFileArg, gotOpts.InputFileArg)
			assert.Equal(t, tt.wants.OutputFile, gotOpts.OutputFile)
			assert.Equal(t, tt.wants.OutputFormat, gotOpts.OutputFormat)
			assert.Equal(t, tt.wants.InputFormat, gotOpts.InputFormat)
		})
	}
}

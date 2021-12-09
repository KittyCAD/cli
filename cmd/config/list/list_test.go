package list

import (
	"bytes"
	"testing"

	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/google/shlex"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/stretchr/testify/assert"
)

func TestNewCmdConfigList(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		output   Options
		wantsErr bool
	}{
		{
			name:     "no arguments",
			input:    "",
			output:   Options{},
			wantsErr: false,
		},
		{
			name:     "list with host",
			input:    "--host HOST.com",
			output:   Options{Hostname: "HOST.com"},
			wantsErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			f := &cli.CLI{
				Config: func() (config.Config, error) {
					return config.Stub{}, nil
				},
			}

			argv, err := shlex.Split(tt.input)
			assert.NoError(t, err)

			var gotOpts *Options
			cmd := NewCmdConfigList(f, func(opts *Options) error {
				gotOpts = opts
				return nil
			})
			cmd.Flags().BoolP("help", "x", false, "")

			cmd.SetArgs(argv)
			cmd.SetIn(&bytes.Buffer{})
			cmd.SetOut(&bytes.Buffer{})
			cmd.SetErr(&bytes.Buffer{})

			_, err = cmd.ExecuteC()
			if tt.wantsErr {
				assert.Error(t, err)
				return
			}

			assert.NoError(t, err)
			assert.Equal(t, tt.output.Hostname, gotOpts.Hostname)
		})
	}
}

func Test_listRun(t *testing.T) {
	tests := []struct {
		name    string
		input   *Options
		config  config.Stub
		stdout  string
		wantErr bool
	}{
		{
			name: "list",
			config: config.Stub{
				"HOST:prompt":  "disabled",
				"HOST:pager":   "less",
				"HOST:browser": "brave",
			},
			input: &Options{Hostname: "HOST"}, // ConfigStub gives empty DefaultHost
			stdout: `prompt=disabled
pager=less
browser=brave
`,
		},
	}

	for _, tt := range tests {
		io, _, stdout, _ := iostreams.Test()
		tt.input.IO = io
		tt.input.Config = func() (config.Config, error) {
			return tt.config, nil
		}

		t.Run(tt.name, func(t *testing.T) {
			err := listRun(tt.input)
			assert.NoError(t, err)
			assert.Equal(t, tt.stdout, stdout.String())
		})
	}
}

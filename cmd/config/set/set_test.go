package set

import (
	"bytes"
	"testing"

	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/google/shlex"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/stretchr/testify/assert"
)

func TestNewCmdConfigSet(t *testing.T) {
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
			wantsErr: true,
		},
		{
			name:     "no value argument",
			input:    "key",
			output:   Options{},
			wantsErr: true,
		},
		{
			name:     "set key value",
			input:    "key value",
			output:   Options{Key: "key", Value: "value"},
			wantsErr: false,
		},
		{
			name:     "set key value with host",
			input:    "key value --host test.com",
			output:   Options{Hostname: "test.com", Key: "key", Value: "value"},
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
			cmd := NewCmdConfigSet(f, func(opts *Options) error {
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
			assert.Equal(t, tt.output.Key, gotOpts.Key)
			assert.Equal(t, tt.output.Value, gotOpts.Value)
		})
	}
}

func Test_setRun(t *testing.T) {
	tests := []struct {
		name          string
		input         *Options
		expectedValue string
		stdout        string
		stderr        string
		wantsErr      bool
		errMsg        string
	}{
		{
			name: "set key value",
			input: &Options{
				Config: config.Stub{},
				Key:    "pager",
				Value:  "more",
			},
			expectedValue: "more",
		},
		{
			name: "set key value scoped by host",
			input: &Options{
				Config:   config.Stub{},
				Hostname: "api.kittycad.io",
				Key:      "pager",
				Value:    "more",
			},
			expectedValue: "more",
		},
		{
			name: "set unknown key",
			input: &Options{
				Config: config.Stub{},
				Key:    "unknownKey",
				Value:  "someValue",
			},
			expectedValue: "someValue",
			stderr:        "! warning: 'unknownKey' is not a known configuration key\n",
		},
		{
			name: "set invalid value",
			input: &Options{
				Config: config.Stub{},
				Key:    "prompt",
				Value:  "invalid",
			},
			wantsErr: true,
			errMsg:   "failed to set \"prompt\" to \"invalid\": valid values are 'enabled', 'disabled'",
		},
	}
	for _, tt := range tests {
		io, _, stdout, stderr := iostreams.Test()
		tt.input.IO = io

		t.Run(tt.name, func(t *testing.T) {
			err := setRun(tt.input)
			if tt.wantsErr {
				assert.EqualError(t, err, tt.errMsg)
				return
			}
			assert.NoError(t, err)
			assert.Equal(t, tt.stdout, stdout.String())
			assert.Equal(t, tt.stderr, stderr.String())

			val, err := tt.input.Config.Get(tt.input.Hostname, tt.input.Key)
			assert.NoError(t, err)
			assert.Equal(t, tt.expectedValue, val)

			val, err = tt.input.Config.Get("", "_written")
			assert.NoError(t, err)
			assert.Equal(t, "true", val)
		})
	}
}

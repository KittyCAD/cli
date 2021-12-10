package status

import (
	"bytes"
	"testing"

	"github.com/google/shlex"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/stretchr/testify/assert"
)

func Test_NewCmdStatus(t *testing.T) {
	tests := []struct {
		name  string
		cli   string
		wants Options
	}{
		{
			name:  "no arguments",
			cli:   "",
			wants: Options{},
		},
		{
			name: "hostname set",
			cli:  "--hostname ellie.williams",
			wants: Options{
				Hostname: "ellie.williams",
			},
		},
		{
			name: "show token",
			cli:  "--show-token",
			wants: Options{
				ShowToken: true,
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			f := &cli.CLI{}

			argv, err := shlex.Split(tt.cli)
			assert.NoError(t, err)

			var gotOpts *Options
			cmd := NewCmdStatus(f, func(opts *Options) error {
				gotOpts = opts
				return nil
			})

			// TODO cobra hack-around
			cmd.Flags().BoolP("help", "x", false, "")

			cmd.SetArgs(argv)
			cmd.SetIn(&bytes.Buffer{})
			cmd.SetOut(&bytes.Buffer{})
			cmd.SetErr(&bytes.Buffer{})

			_, err = cmd.ExecuteC()
			assert.NoError(t, err)

			assert.Equal(t, tt.wants.Hostname, gotOpts.Hostname)
		})
	}
}

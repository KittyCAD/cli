package list

import (
	"bytes"
	"io/ioutil"
	"testing"

	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestAliasList(t *testing.T) {
	tests := []struct {
		name       string
		config     string
		isTTY      bool
		wantStdout string
		wantStderr string
	}{
		{
			name:       "empty",
			config:     "",
			isTTY:      true,
			wantStdout: "",
			wantStderr: "no aliases configured\n",
		},
		{
			name: "some",
			config: heredoc.Doc(`
				aliases:
				  co: pr checkout
				  gc: "!kittycad gist create \"$@\" | pbcopy"
			`),
			isTTY:      true,
			wantStdout: "co:  pr checkout\ngc:  !kittycad gist create \"$@\" | pbcopy\n",
			wantStderr: "",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// TODO: change underlying config implementation so Write is not
			// automatically called when editing aliases in-memory
			defer config.StubWriteConfig(ioutil.Discard, ioutil.Discard)()

			cfg := config.NewFromString(tt.config)

			io, _, stdout, stderr := iostreams.Test()
			io.SetStdoutTTY(tt.isTTY)
			io.SetStdinTTY(tt.isTTY)
			io.SetStderrTTY(tt.isTTY)

			c := &cli.CLI{
				IOStreams: io,
				Config: func() (config.Config, error) {
					return cfg, nil
				},
			}

			cmd := NewCmdList(c, nil)
			cmd.SetArgs([]string{})

			cmd.SetIn(&bytes.Buffer{})
			cmd.SetOut(ioutil.Discard)
			cmd.SetErr(ioutil.Discard)

			_, err := cmd.ExecuteC()
			require.NoError(t, err)

			assert.Equal(t, tt.wantStdout, stdout.String())
			assert.Equal(t, tt.wantStderr, stderr.String())
		})
	}
}

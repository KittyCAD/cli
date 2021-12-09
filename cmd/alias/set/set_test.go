package set

import (
	"bytes"
	"io/ioutil"
	"testing"

	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/cli/cli/v2/test"
	"github.com/google/shlex"
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/pkg/cli"
	"github.com/spf13/cobra"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func runCommand(cfg config.Config, isTTY bool, cmdline string, in string) (*test.CmdOut, error) {
	io, stdin, stdout, stderr := iostreams.Test()
	io.SetStdoutTTY(isTTY)
	io.SetStdinTTY(isTTY)
	io.SetStderrTTY(isTTY)
	stdin.WriteString(in)

	factory := &cli.CLI{
		IOStreams: io,
		Config: func() (config.Config, error) {
			return cfg, nil
		},
	}

	cmd := NewCmdSet(factory, nil)

	// fake command nesting structure needed for validCommand
	rootCmd := &cobra.Command{}
	rootCmd.AddCommand(cmd)
	fileCmd := &cobra.Command{Use: "file"}
	fileCmd.AddCommand(&cobra.Command{Use: "convert"})
	fileCmd.AddCommand(&cobra.Command{Use: "status"})
	rootCmd.AddCommand(fileCmd)
	metaCmd := &cobra.Command{Use: "meta"}
	metaCmd.AddCommand(&cobra.Command{Use: "instance"})
	metaCmd.AddCommand(&cobra.Command{Use: "session"})
	rootCmd.AddCommand(metaCmd)

	argv, err := shlex.Split("set " + cmdline)
	if err != nil {
		return nil, err
	}
	rootCmd.SetArgs(argv)

	rootCmd.SetIn(stdin)
	rootCmd.SetOut(ioutil.Discard)
	rootCmd.SetErr(ioutil.Discard)

	_, err = rootCmd.ExecuteC()
	return &test.CmdOut{
		OutBuf: stdout,
		ErrBuf: stderr,
	}, err
}

func TestAliasSet_kittycad_command(t *testing.T) {
	defer config.StubWriteConfig(ioutil.Discard, ioutil.Discard)()

	cfg := config.NewFromString(``)

	_, err := runCommand(cfg, true, "file 'file status'", "")
	assert.EqualError(t, err, `could not create alias: "file" is already a kittycad command`)
}

func TestAliasSet_empty_aliases(t *testing.T) {
	mainBuf := bytes.Buffer{}
	defer config.StubWriteConfig(&mainBuf, ioutil.Discard)()

	cfg := config.NewFromString(heredoc.Doc(`
		aliases:
		pager: more
	`))

	output, err := runCommand(cfg, true, "fc 'file convert'", "")

	if err != nil {
		t.Fatalf("unexpected error: %s", err)
	}

	//lint:ignore SA1019 prefer using assert.EqualError over ExpectLines
	test.ExpectLines(t, output.Stderr(), "Added alias")

	//lint:ignore SA1019 prefer using assert.EqualError over ExpectLines
	test.ExpectLines(t, output.String(), "")

	expected := `aliases:
    fc: file convert
pager: more
`
	assert.Equal(t, expected, mainBuf.String())
}

func TestAliasSet_existing_alias(t *testing.T) {
	mainBuf := bytes.Buffer{}
	defer config.StubWriteConfig(&mainBuf, ioutil.Discard)()

	cfg := config.NewFromString(heredoc.Doc(`
		aliases:
		  mi: meta session
	`))

	output, err := runCommand(cfg, true, "mi 'meta instance'", "")
	require.NoError(t, err)

	//lint:ignore SA1019 prefer using assert.EqualError over ExpectLines
	test.ExpectLines(t, output.Stderr(), "Changed alias.*mi.*from.*meta session.*to.*meta instance")
}

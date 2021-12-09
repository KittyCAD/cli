package completion

import (
	"strings"
	"testing"

	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/google/shlex"
	"github.com/spf13/cobra"
)

func TestNewCmdCompletion(t *testing.T) {
	tests := []struct {
		name    string
		args    string
		wantOut string
		wantErr string
	}{
		{
			name:    "no arguments",
			args:    "completion",
			wantOut: "complete -o default -F __start_kittycad kittycad",
		},
		{
			name:    "zsh completion",
			args:    "completion -s zsh",
			wantOut: "#compdef _kittycad kittycad",
		},
		{
			name:    "fish completion",
			args:    "completion -s fish",
			wantOut: "complete -c kittycad ",
		},
		{
			name:    "PowerShell completion",
			args:    "completion -s powershell",
			wantOut: "Register-ArgumentCompleter",
		},
		{
			name:    "unsupported shell",
			args:    "completion -s csh",
			wantErr: "unsupported shell type \"csh\"",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			io, _, stdout, stderr := iostreams.Test()
			completeCmd := NewCmdCompletion(io)
			rootCmd := &cobra.Command{Use: "kittycad"}
			rootCmd.AddCommand(completeCmd)

			argv, err := shlex.Split(tt.args)
			if err != nil {
				t.Fatalf("argument splitting error: %v", err)
			}
			rootCmd.SetArgs(argv)
			rootCmd.SetOut(stdout)
			rootCmd.SetErr(stderr)

			_, err = rootCmd.ExecuteC()
			if tt.wantErr != "" {
				if err == nil || err.Error() != tt.wantErr {
					t.Fatalf("expected error %q, got %q", tt.wantErr, err)
				}
				return
			}
			if err != nil {
				t.Fatalf("error executing command: %v", err)
			}

			if !strings.Contains(stdout.String(), tt.wantOut) {
				t.Errorf("completion output did not match:\n%s", stdout.String())
			}
			if len(stderr.String()) > 0 {
				t.Errorf("expected nothing on stderr, got %q", stderr.String())
			}
		})
	}
}

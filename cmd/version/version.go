package version

import (
	"fmt"
	"regexp"
	"strings"

	"github.com/kittycad/cli/pkg/cli"
	"github.com/kittycad/cli/pkg/cmdutil"
	"github.com/kittycad/cli/version"
	"github.com/spf13/cobra"
)

func NewCmdVersion(cli *cli.CLI) *cobra.Command {
	cmd := &cobra.Command{
		Use:    "version",
		Hidden: true,
		Run: func(cmd *cobra.Command, args []string) {
			fmt.Fprint(cli.IOStreams.Out, Format(version.VERSION, version.GITCOMMIT))
		},
	}

	cmdutil.DisableAuthCheck(cmd)

	return cmd
}

func Format(version, gitHash string) string {
	version = strings.TrimPrefix(version, "v")

	var hashStr string
	if gitHash != "" {
		hashStr = fmt.Sprintf(" (%s)", gitHash)
	}

	return fmt.Sprintf("kittycad version %s%s\n%s\n", version, hashStr, changelogURL(version))
}

func changelogURL(version string) string {
	path := "https://github.com/kittycad/cli"
	r := regexp.MustCompile(`^v?\d+\.\d+\.\d+(-[\w.]+)?$`)
	if !r.MatchString(version) {
		return fmt.Sprintf("%s/releases/latest", path)
	}

	url := fmt.Sprintf("%s/releases/tag/v%s", path, strings.TrimPrefix(version, "v"))
	return url
}

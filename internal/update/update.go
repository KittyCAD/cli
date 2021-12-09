package update

import (
	"context"
	"fmt"
	"io/ioutil"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"time"

	"github.com/google/go-github/github"
	"github.com/hashicorp/go-version"
	"gopkg.in/yaml.v3"
)

var gitDescribeSuffixRE = regexp.MustCompile(`\d+-\d+-g[a-f0-9]{8}$`)

type StateEntry struct {
	CheckedForUpdateAt time.Time                `yaml:"checked_for_update_at"`
	LatestRelease      github.RepositoryRelease `yaml:"latest_release"`
}

// CheckForUpdate checks whether this software has had a newer release on GitHub
func CheckForUpdate(ctx context.Context, stateFilePath, repoOwner, repoName, currentVersion string) (*github.RepositoryRelease, error) {
	// Create a new GitHub client.
	client := github.NewClient(nil)

	stateEntry, _ := getStateEntry(stateFilePath)
	if stateEntry != nil && time.Since(stateEntry.CheckedForUpdateAt).Hours() < 24 {
		return nil, nil
	}

	releaseInfo, err := getLatestReleaseInfo(ctx, client, repoOwner, repoName)
	if err != nil {
		return nil, err
	}

	err = setStateEntry(stateFilePath, time.Now(), *releaseInfo)
	if err != nil {
		return nil, err
	}

	if versionGreaterThan(*releaseInfo.TagName, currentVersion) {
		return releaseInfo, nil
	}

	return nil, nil
}

func getLatestReleaseInfo(ctx context.Context, client *github.Client, repoOwner, repoName string) (*github.RepositoryRelease, error) {
	latestRelease, _, err := client.Repositories.GetLatestRelease(ctx, repoOwner, repoName)
	if err != nil {
		return nil, err
	}

	return latestRelease, nil
}

func getStateEntry(stateFilePath string) (*StateEntry, error) {
	content, err := ioutil.ReadFile(stateFilePath)
	if err != nil {
		return nil, err
	}

	var stateEntry StateEntry
	err = yaml.Unmarshal(content, &stateEntry)
	if err != nil {
		return nil, err
	}

	return &stateEntry, nil
}

func setStateEntry(stateFilePath string, t time.Time, r github.RepositoryRelease) error {
	data := StateEntry{CheckedForUpdateAt: t, LatestRelease: r}
	content, err := yaml.Marshal(data)
	if err != nil {
		return err
	}

	err = os.MkdirAll(filepath.Dir(stateFilePath), 0755)
	if err != nil {
		return err
	}

	err = ioutil.WriteFile(stateFilePath, content, 0600)
	return err
}

func versionGreaterThan(v, w string) bool {
	w = gitDescribeSuffixRE.ReplaceAllStringFunc(w, func(m string) string {
		idx := strings.IndexRune(m, '-')
		n, _ := strconv.Atoi(m[0:idx])
		return fmt.Sprintf("%d-pre.0", n+1)
	})

	vv, ve := version.NewVersion(v)
	vw, we := version.NewVersion(w)

	return ve == nil && we == nil && vv.GreaterThan(vw)
}

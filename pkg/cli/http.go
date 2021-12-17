package cli

import (
	"fmt"
	"strings"

	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/version"
	"github.com/kittycad/kittycad.go"
)

type configGetter interface {
	Get(string, string) (string, error)
	DefaultHost() (string, error)
}

// NewKittyCADClient returns an API client for kittycad.io only that borrows from but
// does not depend on user configuration.
// TODO: if they are in debug mode, we should set debug mode in the client library.
func NewKittyCADClient(cfg configGetter, hostname string) (*kittycad.Client, error) {
	if hostname == "" {
		// Get the default hostname from the config.
		var err error
		hostname, err = cfg.DefaultHost()
		if err != nil {
			return nil, fmt.Errorf("error getting default hostname: %v", err)
		}
	}
	token, _ := config.AuthTokenFromEnv(hostname)
	if token == "" {
		token, _ = cfg.Get(hostname, "token")
	}
	client, err := kittycad.NewClient(token, fmt.Sprintf("KittyCAD CLI %s", version.VERSION))
	if err != nil {
		return nil, err
	}

	if hostname == config.KittyCADDefaultHost {
		// Return the default client.
		return client, nil
	}

	// Change the baseURL to the one we want.
	baseurl := fmt.Sprintf("https://%s", hostname)
	if strings.HasPrefix(hostname, "localhost") {
		baseurl = fmt.Sprintf("http://%s", hostname)
	}

	// Set the hostname if it's not the default.
	if err := client.WithBaseURL(baseurl); err != nil {
		return nil, fmt.Errorf("could not set base URL for the client to `%s`: %w", baseurl, err)
	}

	return client, nil
}

package cli

import (
	"fmt"

	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/version"
	"github.com/kittycad/kittycad.go"
)

type configGetter interface {
	Get(string, string) (string, error)
}

// NewKittyCADClient returns an API client for kittycad.io only that borrows from but
// does not depend on user configuration.
// TODO: if this is not the default server, we should set the server properly.
// TODO: if they are in debug mode, we should set debug mode in the client library.
func NewKittyCADClient(cfg configGetter) (*kittycad.Client, error) {
	token, _ := config.AuthTokenFromEnv(config.KittyCADDefaultHost)
	if token == "" {
		token, _ = cfg.Get(config.KittyCADDefaultHost, "token")
	}
	return kittycad.NewClient(token, fmt.Sprintf("KittyCAD CLI %s", version.VERSION))

}

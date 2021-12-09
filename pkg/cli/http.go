package cli

import (
	"github.com/kittycad/cli/internal/config"
	"github.com/kittycad/cli/kittycad"
)

type configGetter interface {
	Get(string, string) (string, error)
}

// NewKittyCADClient returns an API client for kittycad.io only that borrows from but
// does not depend on user configuration.
// TODO: if this is not the default server, we should set the server properly.
// TODO: if they are in debug mode, we should set debug mode in the client library.
func NewKittyCADClient(cfg configGetter) (*kittycad.Client, error) {
	token, _ := config.AuthTokenFromEnv(config.KITTYCAD_DEFAULT_HOST)
	if token == "" {
		token, _ = cfg.Get(config.KITTYCAD_DEFAULT_HOST, "token")
	}
	return kittycad.NewClient(token)

}

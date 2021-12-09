package kittycad

import (
	"fmt"
	"os"

	"github.com/deepmap/oapi-codegen/pkg/securityprovider"
)

// DefaultServerURL is the default server URL for the KittyCad API.
const DefaultServerURL = "https://api.kittycad.io"

// TokenEnvVar is the environment variable that contains the token.
const TokenEnvVar = "KITTYCAD_API_TOKEN"

// NewClient creates a new client for the KittyCad API.
// You need to pass in your API token to create the client.
func NewClient(token string) (*Client, error) {
	if token == "" {
		return nil, fmt.Errorf("you need to pass in an API token to create the client. Create a token at https://kittycad.io/account")
	}

	bearerTokenProvider, err := securityprovider.NewSecurityProviderBearerToken(token)
	if err != nil {
		return nil, fmt.Errorf("failed to create security provider: %s", err)
	}

	client, err := newClient(DefaultServerURL, WithRequestEditorFn(bearerTokenProvider.Intercept))
	if err != nil {
		return nil, fmt.Errorf("failed to create client: %s", err)
	}

	return client, nil
}

// NewClientFromEnv creates a new client for the KittyCad API, using the token
// stored in the environment variable `KITTYCAD_API_TOKEN`.
func NewClientFromEnv() (*Client, error) {
	token := os.Getenv(TokenEnvVar)
	if token == "" {
		return nil, fmt.Errorf("the environment variable %s must be set with your API token. Create a token at https://kittycad.io/account", TokenEnvVar)
	}

	return NewClient(token)
}

package kittycad

import (
	"fmt"
	"os"

	"github.com/deepmap/oapi-codegen/pkg/securityprovider"
)

const KITTYCAD_DEFAULT_SERVER = "https://api.kittycad.io"
const KITTYCAD_TOKEN_ENV_VAR = "KITTYCAD_API_TOKEN"

/// NewDefaultClient creates a new client for the KittyCad API.
/// You need to pass in your API token to create the client.
func NewDefaultClient(token string) (*ClientWithResponses, error) {
	if token == "" {
		return nil, fmt.Errorf("You need to pass in an API token to create the client. Create a token at https://kittycad.io/account")
	}

	bearerTokenProvider, err := securityprovider.NewSecurityProviderBearerToken(token)
	if err != nil {
		return nil, fmt.Errorf("Failed to create security provider: %s", err)
	}

	client, err := NewClientWithResponses(KITTYCAD_DEFAULT_SERVER, WithRequestEditorFn(bearerTokenProvider.Intercept))
	if err != nil {
		return nil, fmt.Errorf("Failed to create client: %s", err)
	}

	return client, nil
}

// NewDefaultClientFromEnv creates a new client for the KittyCad API, using the token
// stored in the environment variable `KITTYCAD_API_TOKEN`.
func NewDefaultClientFromEnv() (*ClientWithResponses, error) {
	token := os.Getenv(KITTYCAD_TOKEN_ENV_VAR)
	if token == "" {
		return nil, fmt.Errorf("The environment variable KITTYCAD_API_TOKEN must be set with your API token. Create a token at https://kittycad.io/account")
	}

	return NewDefaultClient(token)
}

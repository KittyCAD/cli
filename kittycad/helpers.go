package kittycad

import (
	"bytes"
	"context"
	"encoding/base64"
	"fmt"
	"os"

	"github.com/deepmap/oapi-codegen/pkg/securityprovider"
)

// DefaultServerURL is the default server URL for the KittyCad API.
const DefaultServerURL = "http://localhost:8080"

// TokenEnvVar is the environment variable that contains the token.
const TokenEnvVar = "KITTYCAD_API_TOKEN"

// NewClient creates a new client for the KittyCad API.
// You need to pass in your API token to create the client.
func NewClient(token string, opts ...ClientOption) (*Client, error) {
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
func NewClientFromEnv(opts ...ClientOption) (*Client, error) {
	token := os.Getenv(TokenEnvVar)
	if token == "" {
		return nil, fmt.Errorf("the environment variable %s must be set with your API token. Create a token at https://kittycad.io/account", TokenEnvVar)
	}

	return NewClient(token, opts...)
}

// FileConvert converts a file.
func (c *Client) FileConvert(ctx context.Context, srcFormat string, outputFormat string, body []byte) (*FileConversion, []byte, error) {
	var b bytes.Buffer
	encoder := base64.NewEncoder(base64.StdEncoding, &b)
	// Encode the body as base64.
	encoder.Write(body)
	// Must close the encoder when finished to flush any partial blocks.
	// If you comment out the following line, the last partial block "r"
	// won't be encoded.
	encoder.Close()
	resp, err := c.FileConvertWithBody(ctx, ValidFileTypes(srcFormat), ValidFileTypes(outputFormat), "application/json", &b)
	if err != nil {
		return nil, nil, err
	}

	if *resp.Output == "" {
		return resp, nil, nil
	}

	// Decode the base64 encoded body.
	output, err := base64.StdEncoding.DecodeString(*resp.Output)
	if err != nil {
		return nil, nil, fmt.Errorf("base64 decoding output from API failed: %v", err)
	}

	return resp, output, nil
}

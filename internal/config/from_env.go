package config

import (
	"fmt"
	"os"
)

const (
	// KittyCADHostEnvVar is the environment variable name for the host.
	KittyCADHostEnvVar = "KITTYCAD_HOST"
	// KittyCADTokenEnvVar is the environment variable name for the token.
	KittyCADTokenEnvVar = "KITTYCAD_TOKEN"
	// KittyCADAPITokenEnvVar is the environment variable name for the API token.
	KittyCADAPITokenEnvVar = "KITTYCAD_API_TOKEN"
	// KittyCADDefaultHost is the default host.
	KittyCADDefaultHost = "api.kittycad.io"
)

// ReadOnlyEnvError is an error that is returned when an environment is read only.
type ReadOnlyEnvError struct {
	Variable string
}

// Error returns a string representation of the error.
func (e *ReadOnlyEnvError) Error() string {
	return fmt.Sprintf("read-only value in %s", e.Variable)
}

// InheritEnv returns true if the environment variable is inherited.
func InheritEnv(c Config) Config {
	return &envConfig{Config: c}
}

type envConfig struct {
	Config
}

// Hosts returns the list of hosts.
func (c *envConfig) Hosts() ([]string, error) {
	hasDefault := false
	hosts, err := c.Config.Hosts()
	for _, h := range hosts {
		if h == KittyCADDefaultHost {
			hasDefault = true
		}
	}
	token, _ := AuthTokenFromEnv(KittyCADDefaultHost)
	if (err != nil || !hasDefault) && token != "" {
		hosts = append([]string{KittyCADDefaultHost}, hosts...)
		return hosts, nil
	}
	return hosts, err
}

// DefaultHost returns the default host.
func (c *envConfig) DefaultHost() (string, error) {
	val, _, err := c.DefaultHostWithSource()
	return val, err
}

// DefaultHostWithSource returns the default host and the source of the value.
func (c *envConfig) DefaultHostWithSource() (string, string, error) {
	if host := os.Getenv(KittyCADHostEnvVar); host != "" {
		return host, KittyCADHostEnvVar, nil
	}
	return c.Config.DefaultHostWithSource()
}

// Get returns the value for the given key.
func (c *envConfig) Get(hostname, key string) (string, error) {
	val, _, err := c.GetWithSource(hostname, key)
	return val, err
}

// GetWithSource returns the value for the given key and the source of the value.
func (c *envConfig) GetWithSource(hostname, key string) (string, string, error) {
	if hostname != "" && key == "token" {
		if token, env := AuthTokenFromEnv(hostname); token != "" {
			return token, env, nil
		}
	}

	return c.Config.GetWithSource(hostname, key)
}

// CheckWriteable checks if the given key is writeable.
func (c *envConfig) CheckWriteable(hostname, key string) error {
	if hostname != "" && key == "token" {
		if token, env := AuthTokenFromEnv(hostname); token != "" {
			return &ReadOnlyEnvError{Variable: env}
		}
	}

	return c.Config.CheckWriteable(hostname, key)
}

// AuthTokenFromEnv returns the auth token and the environment variable name.
func AuthTokenFromEnv(hostname string) (string, string) {
	if token := os.Getenv(KittyCADTokenEnvVar); token != "" {
		return token, KittyCADTokenEnvVar
	}

	return os.Getenv(KittyCADAPITokenEnvVar), KittyCADAPITokenEnvVar
}

// AuthTokenProvidedFromEnv returns true if the auth token is provided.
func AuthTokenProvidedFromEnv() bool {
	return os.Getenv(KittyCADTokenEnvVar) != "" ||
		os.Getenv(KittyCADAPITokenEnvVar) != ""
}

// IsHostEnv returns true if the environment variable is equal to the source.
func IsHostEnv(src string) bool {
	return src == KittyCADHostEnvVar
}

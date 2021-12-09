package config

import (
	"fmt"
	"os"
)

const (
	KITTYCAD_HOST         = "KITTYCAD_HOST"
	KITTYCAD_TOKEN        = "KITTYCAD_TOKEN"
	KITTYCAD_API_TOKEN    = "KITTYCAD_API_TOKEN"
	KITTYCAD_DEFAULT_HOST = "api.kittycad.io"
)

type ReadOnlyEnvError struct {
	Variable string
}

func (e *ReadOnlyEnvError) Error() string {
	return fmt.Sprintf("read-only value in %s", e.Variable)
}

func InheritEnv(c Config) Config {
	return &envConfig{Config: c}
}

type envConfig struct {
	Config
}

func (c *envConfig) Hosts() ([]string, error) {
	hasDefault := false
	hosts, err := c.Config.Hosts()
	for _, h := range hosts {
		if h == KITTYCAD_DEFAULT_HOST {
			hasDefault = true
		}
	}
	token, _ := AuthTokenFromEnv(KITTYCAD_DEFAULT_HOST)
	if (err != nil || !hasDefault) && token != "" {
		hosts = append([]string{KITTYCAD_DEFAULT_HOST}, hosts...)
		return hosts, nil
	}
	return hosts, err
}

func (c *envConfig) DefaultHost() (string, error) {
	val, _, err := c.DefaultHostWithSource()
	return val, err
}

func (c *envConfig) DefaultHostWithSource() (string, string, error) {
	if host := os.Getenv(KITTYCAD_HOST); host != "" {
		return host, KITTYCAD_HOST, nil
	}
	return c.Config.DefaultHostWithSource()
}

func (c *envConfig) Get(hostname, key string) (string, error) {
	val, _, err := c.GetWithSource(hostname, key)
	return val, err
}

func (c *envConfig) GetWithSource(hostname, key string) (string, string, error) {
	if hostname != "" && key == "token" {
		if token, env := AuthTokenFromEnv(hostname); token != "" {
			return token, env, nil
		}
	}

	return c.Config.GetWithSource(hostname, key)
}

func (c *envConfig) CheckWriteable(hostname, key string) error {
	if hostname != "" && key == "token" {
		if token, env := AuthTokenFromEnv(hostname); token != "" {
			return &ReadOnlyEnvError{Variable: env}
		}
	}

	return c.Config.CheckWriteable(hostname, key)
}

func AuthTokenFromEnv(hostname string) (string, string) {
	if token := os.Getenv(KITTYCAD_TOKEN); token != "" {
		return token, KITTYCAD_TOKEN
	}

	return os.Getenv(KITTYCAD_API_TOKEN), KITTYCAD_API_TOKEN
}

func AuthTokenProvidedFromEnv() bool {
	return os.Getenv(KITTYCAD_TOKEN) != "" ||
		os.Getenv(KITTYCAD_API_TOKEN) != ""
}

func IsHostEnv(src string) bool {
	return src == KITTYCAD_HOST
}

package config

import (
	"errors"
)

// Stub is a stub implementation of the Config interface.
type Stub map[string]string

func genKey(host, key string) string {
	if host != "" {
		return host + ":" + key
	}
	return key
}

// Get returns the value of the given key.
func (c Stub) Get(host, key string) (string, error) {
	val, _, err := c.GetWithSource(host, key)
	return val, err
}

// GetWithSource returns the value of the given key.
func (c Stub) GetWithSource(host, key string) (string, string, error) {
	if v, found := c[genKey(host, key)]; found {
		return v, "(memory)", nil
	}
	return "", "", errors.New("not found")
}

// Set sets the value of the given key.
func (c Stub) Set(host, key, value string) error {
	c[genKey(host, key)] = value
	return nil
}

// Aliases returns the aliases of the given key.
func (c Stub) Aliases() (*AliasConfig, error) {
	return nil, nil
}

// Hosts returns the list of hosts.
func (c Stub) Hosts() ([]string, error) {
	return nil, nil
}

// UnsetHost removes the given host from the list of hosts.
func (c Stub) UnsetHost(hostname string) {
}

// CheckWriteable checks if the config is writeable.
func (c Stub) CheckWriteable(host, key string) error {
	return nil
}

// Write writes the config to the given writer.
func (c Stub) Write() error {
	c["_written"] = "true"
	return nil
}

// DefaultHost returns the default host.
func (c Stub) DefaultHost() (string, error) {
	return "", nil
}

// DefaultHostWithSource returns the default host.
func (c Stub) DefaultHostWithSource() (string, string, error) {
	return "", "", nil
}

package config

import (
	"fmt"
)

// AliasConfig is a config file that stores aliases.
type AliasConfig struct {
	ConfigMap
	Parent Config
}

// Get returns the expansion of the alias.
func (a *AliasConfig) Get(alias string) (string, bool) {
	if a.Empty() {
		return "", false
	}
	value, _ := a.GetStringValue(alias)

	return value, value != ""
}

// Add adds an alias to the config.
func (a *AliasConfig) Add(alias, expansion string) error {
	err := a.SetStringValue(alias, expansion)
	if err != nil {
		return fmt.Errorf("failed to update config: %w", err)
	}

	err = a.Parent.Write()
	if err != nil {
		return fmt.Errorf("failed to write config: %w", err)
	}

	return nil
}

// Delete deletes an alias from the config.
func (a *AliasConfig) Delete(alias string) error {
	a.RemoveEntry(alias)

	err := a.Parent.Write()
	if err != nil {
		return fmt.Errorf("failed to write config: %w", err)
	}

	return nil
}

// All returns all aliases in the config.
func (a *AliasConfig) All() map[string]string {
	out := map[string]string{}

	if a.Empty() {
		return out
	}

	for i := 0; i < len(a.Root.Content)-1; i += 2 {
		key := a.Root.Content[i].Value
		value := a.Root.Content[i+1].Value
		out[key] = value
	}

	return out
}

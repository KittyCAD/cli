package config

import (
	"fmt"

	"gopkg.in/yaml.v3"
)

// Config is the interface for a configuration file.
type Config interface {
	Get(string, string) (string, error)
	GetWithSource(string, string) (string, string, error)
	Set(string, string, string) error
	UnsetHost(string)
	Hosts() ([]string, error)
	DefaultHost() (string, error)
	DefaultHostWithSource() (string, string, error)
	Aliases() (*AliasConfig, error)
	CheckWriteable(string, string) error
	Write() error
}

// Option is a configuration option.
type Option struct {
	Key           string
	Description   string
	DefaultValue  string
	AllowedValues []string
}

var configOptions = []Option{
	{
		Key:           "prompt",
		Description:   "toggle interactive prompting in the terminal",
		DefaultValue:  "enabled",
		AllowedValues: []string{"enabled", "disabled"},
	},
	{
		Key:          "pager",
		Description:  "the terminal pager program to send standard output to",
		DefaultValue: "",
	},
	{
		Key:          "browser",
		Description:  "the web browser to use for opening URLs",
		DefaultValue: "",
	},
}

// Options returns a list of all config options.
func Options() []Option {
	return configOptions
}

// ValidateKey validates a key.
func ValidateKey(key string) error {
	for _, configKey := range configOptions {
		if key == configKey.Key {
			return nil
		}
	}

	return fmt.Errorf("invalid key")
}

// InvalidValueError is an error that occurs when a value is not valid for a key.
type InvalidValueError struct {
	ValidValues []string
}

// Error implements the error interface.
func (e InvalidValueError) Error() string {
	return "invalid value"
}

// ValidateValue validates a value for a key.
func ValidateValue(key, value string) error {
	var validValues []string

	for _, v := range configOptions {
		if v.Key == key {
			validValues = v.AllowedValues
			break
		}
	}

	if validValues == nil {
		return nil
	}

	for _, v := range validValues {
		if v == value {
			return nil
		}
	}

	return &InvalidValueError{ValidValues: validValues}
}

// NewConfig initializes a Config from a yaml node.
func NewConfig(root *yaml.Node) Config {
	return &fileConfig{
		ConfigMap:    ConfigMap{Root: root.Content[0]},
		documentRoot: root,
	}
}

// NewFromString initializes a Config from a yaml string.
func NewFromString(str string) Config {
	root, err := parseConfigData([]byte(str))
	if err != nil {
		panic(err)
	}
	return NewConfig(root)
}

// NewBlankConfig initializes a config file pre-populated with comments and default values.
func NewBlankConfig() Config {
	return NewConfig(NewBlankRoot())
}

// NewBlankRoot initializes a config file pre-populated with comments and default values.
func NewBlankRoot() *yaml.Node {
	return &yaml.Node{
		Kind: yaml.DocumentNode,
		Content: []*yaml.Node{
			{
				Kind: yaml.MappingNode,
				Content: []*yaml.Node{
					{
						HeadComment: "When to interactively prompt. This is a global config that cannot be overridden by hostname. Supported values: enabled, disabled",
						Kind:        yaml.ScalarNode,
						Value:       "prompt",
					},
					{
						Kind:  yaml.ScalarNode,
						Value: "enabled",
					},
					{
						HeadComment: "A pager program to send command output to, e.g. \"less\". Set the value to \"cat\" to disable the pager.",
						Kind:        yaml.ScalarNode,
						Value:       "pager",
					},
					{
						Kind:  yaml.ScalarNode,
						Value: "",
					},
					{
						HeadComment: "Aliases allow you to create nicknames for kittycad commands",
						Kind:        yaml.ScalarNode,
						Value:       "aliases",
					},
					{
						Kind: yaml.MappingNode,
						Content: []*yaml.Node{
							{
								Kind:  yaml.ScalarNode,
								Value: "co",
							},
							{
								Kind:  yaml.ScalarNode,
								Value: "file convert",
							},
						},
					},
					{
						HeadComment: "What web browser kittycad should use when opening URLs. If blank, will refer to environment.",
						Kind:        yaml.ScalarNode,
						Value:       "browser",
					},
					{
						Kind:  yaml.ScalarNode,
						Value: "",
					},
				},
			},
		},
	}
}

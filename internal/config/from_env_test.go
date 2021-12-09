package config

import (
	"os"
	"testing"

	"github.com/MakeNowJust/heredoc"
	"github.com/stretchr/testify/assert"
)

func setenv(t *testing.T, key, newValue string) {
	oldValue, hasValue := os.LookupEnv(key)
	os.Setenv(key, newValue)
	t.Cleanup(func() {
		if hasValue {
			os.Setenv(key, oldValue)
		} else {
			os.Unsetenv(key)
		}
	})
}

func TestInheritEnv(t *testing.T) {
	ogKittyCADAPITokenEnvVar := os.Getenv("KITTYCAD_API_TOKEN")
	ogKittyCADTokenEnvVar := os.Getenv("KITTYCAD_TOKEN")
	ogAppData := os.Getenv("AppData")
	t.Cleanup(func() {
		os.Setenv("KITTYCAD_API_TOKEN", ogKittyCADAPITokenEnvVar)
		os.Setenv("KITTYCAD_TOKEN", ogKittyCADTokenEnvVar)
		os.Setenv("AppData", ogAppData)
	})

	type wants struct {
		hosts     []string
		token     string
		source    string
		writeable bool
	}

	tests := []struct {
		name                   string
		baseConfig             string
		KittyCADAPITokenEnvVar string
		KittyCADTokenEnvVar    string
		hostname               string
		wants                  wants
	}{
		{
			name:       "blank",
			baseConfig: ``,
			hostname:   "api.kittycad.io",
			wants: wants{
				hosts:     []string{},
				token:     "",
				source:    ".config.kittycad.config.yml",
				writeable: true,
			},
		},
		{
			name:                   "KITTYCAD_API_TOKEN over blank config",
			baseConfig:             ``,
			KittyCADAPITokenEnvVar: "OTOKEN",
			hostname:               "api.kittycad.io",
			wants: wants{
				hosts:     []string{"api.kittycad.io"},
				token:     "OTOKEN",
				source:    "KITTYCAD_API_TOKEN",
				writeable: false,
			},
		},
		{
			name:                "KITTYCAD_TOKEN over blank config",
			baseConfig:          ``,
			KittyCADTokenEnvVar: "OTOKEN",
			hostname:            "api.kittycad.io",
			wants: wants{
				hosts:     []string{"api.kittycad.io"},
				token:     "OTOKEN",
				source:    "KITTYCAD_TOKEN",
				writeable: false,
			},
		},
		{
			name: "token from file",
			baseConfig: heredoc.Doc(`
			hosts:
			  api.kittycad.io:
			    token: OTOKEN
			`),
			hostname: "api.kittycad.io",
			wants: wants{
				hosts:     []string{"api.kittycad.io"},
				token:     "OTOKEN",
				source:    ".config.kittycad.hosts.yml",
				writeable: true,
			},
		},
		{
			name: "KITTYCAD_API_TOKEN shadows token from file",
			baseConfig: heredoc.Doc(`
			hosts:
			  api.kittycad.io:
			    token: OTOKEN
			`),
			KittyCADAPITokenEnvVar: "ENVTOKEN",
			hostname:               "api.kittycad.io",
			wants: wants{
				hosts:     []string{"api.kittycad.io"},
				token:     "ENVTOKEN",
				source:    "KITTYCAD_API_TOKEN",
				writeable: false,
			},
		},
		{
			name: "KITTYCAD_TOKEN shadows token from file",
			baseConfig: heredoc.Doc(`
			hosts:
			  api.kittycad.io:
			    token: OTOKEN
			`),
			KittyCADTokenEnvVar: "ENVTOKEN",
			hostname:            "api.kittycad.io",
			wants: wants{
				hosts:     []string{"api.kittycad.io"},
				token:     "ENVTOKEN",
				source:    "KITTYCAD_TOKEN",
				writeable: false,
			},
		},
		{
			name:                   "KITTYCAD_TOKEN shadows token from KITTYCAD_API_TOKEN",
			baseConfig:             ``,
			KittyCADTokenEnvVar:    "GHTOKEN",
			KittyCADAPITokenEnvVar: "GITHUBTOKEN",
			hostname:               "api.kittycad.io",
			wants: wants{
				hosts:     []string{"api.kittycad.io"},
				token:     "GHTOKEN",
				source:    "KITTYCAD_TOKEN",
				writeable: false,
			},
		},
		{
			name: "KITTYCAD_API_TOKEN adds host entry",
			baseConfig: heredoc.Doc(`
			hosts:
			  example.org:
			    token: OTOKEN
			`),
			KittyCADAPITokenEnvVar: "ENVTOKEN",
			hostname:               "api.kittycad.io",
			wants: wants{
				hosts:     []string{"api.kittycad.io", "example.org"},
				token:     "ENVTOKEN",
				source:    "KITTYCAD_API_TOKEN",
				writeable: false,
			},
		},
		{
			name: "KITTYCAD_TOKEN adds host entry",
			baseConfig: heredoc.Doc(`
			hosts:
			  example.org:
			    token: OTOKEN
			`),
			KittyCADTokenEnvVar: "ENVTOKEN",
			hostname:            "api.kittycad.io",
			wants: wants{
				hosts:     []string{"api.kittycad.io", "example.org"},
				token:     "ENVTOKEN",
				source:    "KITTYCAD_TOKEN",
				writeable: false,
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			setenv(t, "KITTYCAD_API_TOKEN", tt.KittyCADAPITokenEnvVar)
			setenv(t, "KITTYCAD_TOKEN", tt.KittyCADTokenEnvVar)
			setenv(t, "AppData", "")

			baseCfg := NewFromString(tt.baseConfig)
			cfg := InheritEnv(baseCfg)

			hosts, _ := cfg.Hosts()
			assert.Equal(t, tt.wants.hosts, hosts)

			val, source, _ := cfg.GetWithSource(tt.hostname, "token")
			assert.Equal(t, tt.wants.token, val)
			assert.Regexp(t, tt.wants.source, source)

			val, _ = cfg.Get(tt.hostname, "token")
			assert.Equal(t, tt.wants.token, val)

			err := cfg.CheckWriteable(tt.hostname, "token")
			if tt.wants.writeable != (err == nil) {
				t.Errorf("CheckWriteable() = %v, wants %v", err, tt.wants.writeable)
			}
		})
	}
}

func TestAuthTokenProvidedFromEnv(t *testing.T) {
	ogKittyCADAPITokenEnvVar := os.Getenv("KITTYCAD_API_TOKEN")
	ogKittyCADTokenEnvVar := os.Getenv("KITTYCAD_TOKEN")
	t.Cleanup(func() {
		os.Setenv("KITTYCAD_API_TOKEN", ogKittyCADAPITokenEnvVar)
		os.Setenv("KITTYCAD_TOKEN", ogKittyCADTokenEnvVar)
	})

	tests := []struct {
		name                   string
		KittyCADAPITokenEnvVar string
		KittyCADTokenEnvVar    string
		provided               bool
	}{
		{
			name:     "no env tokens",
			provided: false,
		},
		{
			name:                "KITTYCAD_TOKEN",
			KittyCADTokenEnvVar: "TOKEN",
			provided:            true,
		},
		{
			name:                   "KITTYCAD_API_TOKEN",
			KittyCADAPITokenEnvVar: "TOKEN",
			provided:               true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			os.Setenv("KITTYCAD_API_TOKEN", tt.KittyCADAPITokenEnvVar)
			os.Setenv("KITTYCAD_TOKEN", tt.KittyCADTokenEnvVar)
			assert.Equal(t, tt.provided, AuthTokenProvidedFromEnv())
		})
	}
}

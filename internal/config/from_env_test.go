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
	orig_KITTYCAD_API_TOKEN := os.Getenv("KITTYCAD_API_TOKEN")
	orig_KITTYCAD_TOKEN := os.Getenv("KITTYCAD_TOKEN")
	orig_AppData := os.Getenv("AppData")
	t.Cleanup(func() {
		os.Setenv("KITTYCAD_API_TOKEN", orig_KITTYCAD_API_TOKEN)
		os.Setenv("KITTYCAD_TOKEN", orig_KITTYCAD_TOKEN)
		os.Setenv("AppData", orig_AppData)
	})

	type wants struct {
		hosts     []string
		token     string
		source    string
		writeable bool
	}

	tests := []struct {
		name               string
		baseConfig         string
		KITTYCAD_API_TOKEN string
		KITTYCAD_TOKEN     string
		hostname           string
		wants              wants
	}{
		{
			name:       "blank",
			baseConfig: ``,
			hostname:   "api.kittycad.io",
			wants: wants{
				hosts:     []string{},
				token:     "",
				source:    ".config.gh.config.yml",
				writeable: true,
			},
		},
		{
			name:               "KITTYCAD_API_TOKEN over blank config",
			baseConfig:         ``,
			KITTYCAD_API_TOKEN: "OTOKEN",
			hostname:           "api.kittycad.io",
			wants: wants{
				hosts:     []string{"api.kittycad.io"},
				token:     "OTOKEN",
				source:    "KITTYCAD_API_TOKEN",
				writeable: false,
			},
		},
		{
			name:           "KITTYCAD_TOKEN over blank config",
			baseConfig:     ``,
			KITTYCAD_TOKEN: "OTOKEN",
			hostname:       "api.kittycad.io",
			wants: wants{
				hosts:     []string{"api.kittycad.io"},
				token:     "OTOKEN",
				source:    "KITTYCAD_TOKEN",
				writeable: false,
			},
		},
		{
			name:               "KITTYCAD_API_TOKEN not applicable to GHE",
			baseConfig:         ``,
			KITTYCAD_API_TOKEN: "OTOKEN",
			hostname:           "example.org",
			wants: wants{
				hosts:     []string{"api.kittycad.io"},
				token:     "",
				source:    ".config.gh.config.yml",
				writeable: true,
			},
		},
		{
			name:           "KITTYCAD_TOKEN not applicable to GHE",
			baseConfig:     ``,
			KITTYCAD_TOKEN: "OTOKEN",
			hostname:       "example.org",
			wants: wants{
				hosts:     []string{"api.kittycad.io"},
				token:     "",
				source:    ".config.gh.config.yml",
				writeable: true,
			},
		},
		{
			name:               "KITTYCAD_API_TOKEN allowed in Codespaces",
			baseConfig:         ``,
			KITTYCAD_API_TOKEN: "OTOKEN",
			hostname:           "example.org",
			wants: wants{
				hosts:     []string{"api.kittycad.io"},
				token:     "OTOKEN",
				source:    "KITTYCAD_API_TOKEN",
				writeable: false,
			},
		},
		{
			name: "token from file",
			baseConfig: heredoc.Doc(`
			hosts:
			  github.com:
			    token: OTOKEN
			`),
			hostname: "api.kittycad.io",
			wants: wants{
				hosts:     []string{"api.kittycad.io"},
				token:     "OTOKEN",
				source:    ".config.gh.hosts.yml",
				writeable: true,
			},
		},
		{
			name: "KITTYCAD_API_TOKEN shadows token from file",
			baseConfig: heredoc.Doc(`
			hosts:
			  github.com:
			    token: OTOKEN
			`),
			KITTYCAD_API_TOKEN: "ENVTOKEN",
			hostname:           "api.kittycad.io",
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
			  github.com:
			    token: OTOKEN
			`),
			KITTYCAD_TOKEN: "ENVTOKEN",
			hostname:       "api.kittycad.io",
			wants: wants{
				hosts:     []string{"api.kittycad.io"},
				token:     "ENVTOKEN",
				source:    "KITTYCAD_TOKEN",
				writeable: false,
			},
		},
		{
			name:               "KITTYCAD_TOKEN shadows token from KITTYCAD_API_TOKEN",
			baseConfig:         ``,
			KITTYCAD_TOKEN:     "GHTOKEN",
			KITTYCAD_API_TOKEN: "GITHUBTOKEN",
			hostname:           "api.kittycad.io",
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
			KITTYCAD_API_TOKEN: "ENVTOKEN",
			hostname:           "api.kittycad.io",
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
			KITTYCAD_TOKEN: "ENVTOKEN",
			hostname:       "api.kittycad.io",
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
			setenv(t, "KITTYCAD_API_TOKEN", tt.KITTYCAD_API_TOKEN)
			setenv(t, "KITTYCAD_TOKEN", tt.KITTYCAD_TOKEN)
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
	orig_KITTYCAD_API_TOKEN := os.Getenv("KITTYCAD_API_TOKEN")
	orig_KITTYCAD_TOKEN := os.Getenv("KITTYCAD_TOKEN")
	t.Cleanup(func() {
		os.Setenv("KITTYCAD_API_TOKEN", orig_KITTYCAD_API_TOKEN)
		os.Setenv("KITTYCAD_TOKEN", orig_KITTYCAD_TOKEN)
	})

	tests := []struct {
		name               string
		KITTYCAD_API_TOKEN string
		KITTYCAD_TOKEN     string
		provided           bool
	}{
		{
			name:     "no env tokens",
			provided: false,
		},
		{
			name:           "KITTYCAD_TOKEN",
			KITTYCAD_TOKEN: "TOKEN",
			provided:       true,
		},
		{
			name:               "KITTYCAD_API_TOKEN",
			KITTYCAD_API_TOKEN: "TOKEN",
			provided:           true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			os.Setenv("KITTYCAD_API_TOKEN", tt.KITTYCAD_API_TOKEN)
			os.Setenv("KITTYCAD_TOKEN", tt.KITTYCAD_TOKEN)
			assert.Equal(t, tt.provided, AuthTokenProvidedFromEnv())
		})
	}
}

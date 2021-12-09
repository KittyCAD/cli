package cmdutil

import (
	"os"
	"testing"

	"github.com/kittycad/cli/internal/config"
	"github.com/stretchr/testify/assert"
)

func Test_CheckAuth(t *testing.T) {
	orig_KITTYCAD_TOKEN := os.Getenv("KITTYCAD_TOKEN")
	t.Cleanup(func() {
		os.Setenv("KITTYCAD_TOKEN", orig_KITTYCAD_TOKEN)
	})

	tests := []struct {
		name     string
		cfg      func(config.Config)
		envToken bool
		expected bool
	}{
		{
			name:     "no hosts",
			cfg:      func(c config.Config) {},
			envToken: false,
			expected: false,
		},
		{name: "no hosts, env auth token",
			cfg:      func(c config.Config) {},
			envToken: true,
			expected: true,
		},
		{
			name: "host, no token",
			cfg: func(c config.Config) {
				_ = c.Set("api.kittycad.io", "token", "")
			},
			envToken: false,
			expected: false,
		},
		{
			name: "host, token",
			cfg: func(c config.Config) {
				_ = c.Set("api.kittycad.io", "token", "a token")
			},
			envToken: false,
			expected: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.envToken {
				os.Setenv("KITTYCAD_TOKEN", "TOKEN")
			} else {
				os.Setenv("KITTYCAD_TOKEN", "")
			}

			cfg := config.NewBlankConfig()
			tt.cfg(cfg)
			result := CheckAuth(cfg)
			assert.Equal(t, tt.expected, result)
		})
	}
}

package cli

import (
	"context"
	"os"
	"testing"

	"github.com/go-playground/assert/v2"
	"github.com/kittycad/cli/internal/config"
)

func Test_ioStreams_pager(t *testing.T) {
	tests := []struct {
		name      string
		env       map[string]string
		config    config.Config
		wantPager string
	}{
		{
			name: "KITTYCAD_PAGER and PAGER set",
			env: map[string]string{
				"KITTYCAD_PAGER": "KITTYCAD_PAGER",
				"PAGER":          "PAGER",
			},
			wantPager: "KITTYCAD_PAGER",
		},
		{
			name: "KITTYCAD_PAGER and config pager set",
			env: map[string]string{
				"KITTYCAD_PAGER": "KITTYCAD_PAGER",
			},
			config:    pagerConfig(),
			wantPager: "KITTYCAD_PAGER",
		},
		{
			name: "config pager and PAGER set",
			env: map[string]string{
				"PAGER": "PAGER",
			},
			config:    pagerConfig(),
			wantPager: "CONFIG_PAGER",
		},
		{
			name: "only PAGER set",
			env: map[string]string{
				"PAGER": "PAGER",
			},
			wantPager: "PAGER",
		},
		{
			name: "KITTYCAD_PAGER set to blank string",
			env: map[string]string{
				"KITTYCAD_PAGER": "",
				"PAGER":          "PAGER",
			},
			wantPager: "",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.env != nil {
				for k, v := range tt.env {
					old := os.Getenv(k)
					os.Setenv(k, v)
					if k == "KITTYCAD_PAGER" {
						defer os.Unsetenv(k)
					} else {
						defer os.Setenv(k, old)
					}
				}
			}
			ctx := context.Background()
			f := New(ctx)
			f.Config = func() (config.Config, error) {
				if tt.config == nil {
					return config.NewBlankConfig(), nil
				}
				return tt.config, nil
			}
			io := ioStreams(f)
			assert.Equal(t, tt.wantPager, io.GetPager())
		})
	}
}

func Test_ioStreams_prompt(t *testing.T) {
	tests := []struct {
		name           string
		config         config.Config
		promptDisabled bool
	}{
		{
			name:           "default config",
			promptDisabled: false,
		},
		{
			name:           "config with prompt disabled",
			config:         disablePromptConfig(),
			promptDisabled: true,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ctx := context.Background()
			f := New(ctx)
			f.Config = func() (config.Config, error) {
				if tt.config == nil {
					return config.NewBlankConfig(), nil
				}
				return tt.config, nil
			}
			io := ioStreams(f)
			assert.Equal(t, tt.promptDisabled, io.GetNeverPrompt())
		})
	}
}

func Test_browserLauncher(t *testing.T) {
	tests := []struct {
		name        string
		env         map[string]string
		config      config.Config
		wantBrowser string
	}{
		{
			name: "KITTYCAD_BROWSER set",
			env: map[string]string{
				"KITTYCAD_BROWSER": "KITTYCAD_BROWSER",
			},
			wantBrowser: "KITTYCAD_BROWSER",
		},
		{
			name:        "config browser set",
			config:      config.NewFromString("browser: CONFIG_BROWSER"),
			wantBrowser: "CONFIG_BROWSER",
		},
		{
			name: "BROWSER set",
			env: map[string]string{
				"BROWSER": "BROWSER",
			},
			wantBrowser: "BROWSER",
		},
		{
			name: "KITTYCAD_BROWSER and config browser set",
			env: map[string]string{
				"KITTYCAD_BROWSER": "KITTYCAD_BROWSER",
			},
			config:      config.NewFromString("browser: CONFIG_BROWSER"),
			wantBrowser: "KITTYCAD_BROWSER",
		},
		{
			name: "config browser and BROWSER set",
			env: map[string]string{
				"BROWSER": "BROWSER",
			},
			config:      config.NewFromString("browser: CONFIG_BROWSER"),
			wantBrowser: "CONFIG_BROWSER",
		},
		{
			name: "KITTYCAD_BROWSER and BROWSER set",
			env: map[string]string{
				"BROWSER":          "BROWSER",
				"KITTYCAD_BROWSER": "KITTYCAD_BROWSER",
			},
			wantBrowser: "KITTYCAD_BROWSER",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.env != nil {
				for k, v := range tt.env {
					old := os.Getenv(k)
					os.Setenv(k, v)
					defer os.Setenv(k, old)
				}
			}
			ctx := context.Background()
			f := New(ctx)
			f.Config = func() (config.Config, error) {
				if tt.config == nil {
					return config.NewBlankConfig(), nil
				}
				return tt.config, nil
			}
			browser := browserLauncher(f)
			assert.Equal(t, tt.wantBrowser, browser)
		})
	}
}

func pagerConfig() config.Config {
	return config.NewFromString("pager: CONFIG_PAGER")
}

func disablePromptConfig() config.Config {
	return config.NewFromString("prompt: disabled")
}

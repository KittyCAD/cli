package config

import (
	"fmt"
	"io/ioutil"
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/stretchr/testify/assert"
	"gopkg.in/yaml.v3"
)

func Test_parseConfig(t *testing.T) {
	defer stubConfig(`---
hosts:
  api.kittycad.io:
    token: BLAH
`, "")()
	config, err := parseConfig("config.yml")
	assert.NoError(t, err)
	token, err := config.Get("api.kittycad.io", "token")
	assert.NoError(t, err)
	assert.Equal(t, "BLAH", token)
}

func Test_parseConfig_multipleHosts(t *testing.T) {
	defer stubConfig(`---
hosts:
  not.kittycad.io:
    token: NOTTHIS
  api.kittycad.io:
    token: BLAH
`, "")()
	config, err := parseConfig("config.yml")
	assert.NoError(t, err)
	token, err := config.Get("api.kittycad.io", "token")
	assert.NoError(t, err)
	assert.Equal(t, "BLAH", token)
}

func Test_parseConfig_hostsFile(t *testing.T) {
	defer stubConfig("", `---
api.kittycad.io:
  token: BLAH
`)()
	config, err := parseConfig("config.yml")
	assert.NoError(t, err)
	token, err := config.Get("api.kittycad.io", "token")
	assert.NoError(t, err)
	assert.Equal(t, "BLAH", token)
}

func Test_parseConfigFile(t *testing.T) {
	tests := []struct {
		contents string
		wantsErr bool
	}{
		{
			contents: "",
			wantsErr: true,
		},
		{
			contents: " ",
			wantsErr: false,
		},
		{
			contents: "\n",
			wantsErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(fmt.Sprintf("contents: %q", tt.contents), func(t *testing.T) {
			defer stubConfig(tt.contents, "")()
			_, yamlRoot, err := parseConfigFile("config.yml")
			if tt.wantsErr != (err != nil) {
				t.Fatalf("got error: %v", err)
			}
			if tt.wantsErr {
				return
			}
			assert.Equal(t, yaml.MappingNode, yamlRoot.Content[0].Kind)
			assert.Equal(t, 0, len(yamlRoot.Content[0].Content))
		})
	}
}

func Test_ConfigDir(t *testing.T) {
	tempDir := t.TempDir()

	tests := []struct {
		name        string
		onlyWindows bool
		env         map[string]string
		output      string
	}{
		{
			name: "HOME/USERPROFILE specified",
			env: map[string]string{
				"KITTYCAD_CONFIG_DIR": "",
				"XDG_CONFIG_HOME":     "",
				"AppData":             "",
				"USERPROFILE":         tempDir,
				"HOME":                tempDir,
			},
			output: filepath.Join(tempDir, ".config", "kittycad"),
		},
		{
			name: "KITTYCAD_CONFIG_DIR specified",
			env: map[string]string{
				"KITTYCAD_CONFIG_DIR": filepath.Join(tempDir, "kittycad_config_dir"),
			},
			output: filepath.Join(tempDir, "kittycad_config_dir"),
		},
		{
			name: "XDG_CONFIG_HOME specified",
			env: map[string]string{
				"XDG_CONFIG_HOME": tempDir,
			},
			output: filepath.Join(tempDir, "kittycad"),
		},
		{
			name: "GH_CONFIG_DIR and XDG_CONFIG_HOME specified",
			env: map[string]string{
				"GH_CONFIG_DIR":   filepath.Join(tempDir, "kittycad_config_dir"),
				"XDG_CONFIG_HOME": tempDir,
			},
			output: filepath.Join(tempDir, "kittycad_config_dir"),
		},
		{
			name:        "AppData specified",
			onlyWindows: true,
			env: map[string]string{
				"AppData": tempDir,
			},
			output: filepath.Join(tempDir, "KittyCAD CLI"),
		},
		{
			name:        "GH_CONFIG_DIR and AppData specified",
			onlyWindows: true,
			env: map[string]string{
				"GH_CONFIG_DIR": filepath.Join(tempDir, "gh_config_dir"),
				"AppData":       tempDir,
			},
			output: filepath.Join(tempDir, "gh_config_dir"),
		},
		{
			name:        "XDG_CONFIG_HOME and AppData specified",
			onlyWindows: true,
			env: map[string]string{
				"XDG_CONFIG_HOME": tempDir,
				"AppData":         tempDir,
			},
			output: filepath.Join(tempDir, "kittycad"),
		},
	}

	for _, tt := range tests {
		if tt.onlyWindows && runtime.GOOS != "windows" {
			continue
		}
		t.Run(tt.name, func(t *testing.T) {
			if tt.env != nil {
				for k, v := range tt.env {
					old := os.Getenv(k)
					os.Setenv(k, v)
					defer os.Setenv(k, old)
				}
			}

			// Create directory to skip auto migration code
			// which gets run when target directory does not exist
			_ = os.MkdirAll(tt.output, 0755)

			assert.Equal(t, tt.output, Dir())
		})
	}
}

func Test_configFile_Write_toDisk(t *testing.T) {
	configDir := filepath.Join(t.TempDir(), ".config", "kittycad")
	_ = os.MkdirAll(configDir, 0755)
	os.Setenv(GH_CONFIG_DIR, configDir)
	defer os.Unsetenv(GH_CONFIG_DIR)

	cfg := NewFromString(`pager: less`)
	err := cfg.Write()
	if err != nil {
		t.Fatal(err)
	}

	expectedConfig := "pager: less\n"
	if configBytes, err := ioutil.ReadFile(filepath.Join(configDir, "config.yml")); err != nil {
		t.Error(err)
	} else if string(configBytes) != expectedConfig {
		t.Errorf("expected config.yml %q, got %q", expectedConfig, string(configBytes))
	}

	if configBytes, err := ioutil.ReadFile(filepath.Join(configDir, "hosts.yml")); err != nil {
		t.Error(err)
	} else if string(configBytes) != "" {
		t.Errorf("unexpected hosts.yml: %q", string(configBytes))
	}
}

func Test_StateDir(t *testing.T) {
	tempDir := t.TempDir()

	tests := []struct {
		name        string
		onlyWindows bool
		env         map[string]string
		output      string
	}{
		{
			name: "HOME/USERPROFILE specified",
			env: map[string]string{
				"XDG_STATE_HOME":  "",
				"GH_CONFIG_DIR":   "",
				"XDG_CONFIG_HOME": "",
				"LocalAppData":    "",
				"USERPROFILE":     tempDir,
				"HOME":            tempDir,
			},
			output: filepath.Join(tempDir, ".local", "state", "kittycad"),
		},
		{
			name: "XDG_STATE_HOME specified",
			env: map[string]string{
				"XDG_STATE_HOME": tempDir,
			},
			output: filepath.Join(tempDir, "kittycad"),
		},
		{
			name:        "LocalAppData specified",
			onlyWindows: true,
			env: map[string]string{
				"LocalAppData": tempDir,
			},
			output: filepath.Join(tempDir, "KittyCAD CLI"),
		},
		{
			name:        "XDG_STATE_HOME and LocalAppData specified",
			onlyWindows: true,
			env: map[string]string{
				"XDG_STATE_HOME": tempDir,
				"LocalAppData":   tempDir,
			},
			output: filepath.Join(tempDir, "kittycad"),
		},
	}

	for _, tt := range tests {
		if tt.onlyWindows && runtime.GOOS != "windows" {
			continue
		}
		t.Run(tt.name, func(t *testing.T) {
			if tt.env != nil {
				for k, v := range tt.env {
					old := os.Getenv(k)
					os.Setenv(k, v)
					defer os.Setenv(k, old)
				}
			}

			// Create directory to skip auto migration code
			// which gets run when target directory does not exist
			_ = os.MkdirAll(tt.output, 0755)

			assert.Equal(t, tt.output, StateDir())
		})
	}
}

func Test_DataDir(t *testing.T) {
	tempDir := t.TempDir()

	tests := []struct {
		name        string
		onlyWindows bool
		env         map[string]string
		output      string
	}{
		{
			name: "HOME/USERPROFILE specified",
			env: map[string]string{
				"XDG_DATA_HOME":   "",
				"GH_CONFIG_DIR":   "",
				"XDG_CONFIG_HOME": "",
				"LocalAppData":    "",
				"USERPROFILE":     tempDir,
				"HOME":            tempDir,
			},
			output: filepath.Join(tempDir, ".local", "share", "kittycad"),
		},
		{
			name: "XDG_DATA_HOME specified",
			env: map[string]string{
				"XDG_DATA_HOME": tempDir,
			},
			output: filepath.Join(tempDir, "kittycad"),
		},
		{
			name:        "LocalAppData specified",
			onlyWindows: true,
			env: map[string]string{
				"LocalAppData": tempDir,
			},
			output: filepath.Join(tempDir, "KittyCAD CLI"),
		},
		{
			name:        "XDG_DATA_HOME and LocalAppData specified",
			onlyWindows: true,
			env: map[string]string{
				"XDG_DATA_HOME": tempDir,
				"LocalAppData":  tempDir,
			},
			output: filepath.Join(tempDir, "kittycad"),
		},
	}

	for _, tt := range tests {
		if tt.onlyWindows && runtime.GOOS != "windows" {
			continue
		}
		t.Run(tt.name, func(t *testing.T) {
			if tt.env != nil {
				for k, v := range tt.env {
					old := os.Getenv(k)
					os.Setenv(k, v)
					defer os.Setenv(k, old)
				}
			}

			assert.Equal(t, tt.output, DataDir())
		})
	}
}

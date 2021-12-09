package config

import (
	"errors"
	"fmt"
	"io/ioutil"
	"os"
	"path/filepath"
	"runtime"
	"syscall"

	"gopkg.in/yaml.v3"
)

const (
	// KittyCADConfigDir is the environment variable that can be used to override
	// the default config directory.
	KittyCADConfigDir = "KITTYCAD_CONFIG_DIR"
	// KittyCADShorthandDir is the directory name used for the config and state.
	KittyCADShorthandDir = "kittycad"
	// KittyCADWindowsDir is the directory name used for the config and state on Windows.`
	KittyCADWindowsDir = "KittyCAD CLI"
	// XDGConfigHome is the environment variable for the XDG_CONFIG_HOME.
	XDGConfigHome = "XDG_CONFIG_HOME"
	// XDGStateHome is the environment variable for the XDG_STATE_HOME.
	XDGStateHome = "XDG_STATE_HOME"
	// XDGDataHome is the environment variable for the XDG_DATA_HOME.
	XDGDataHome = "XDG_DATA_HOME"
	// AppData is the directory for AppData on Windows.
	AppData = "AppData"
	// LocalAppData is the directory for LocalAppData on Windows.
	LocalAppData = "LocalAppData"
)

// Dir returns the path to the config directory.
// Config path precedence
// 1. KITTYCAD_CONFIG_DIR
// 2. XDG_CONFIG_HOME
// 3. AppData (windows only)
// 4. HOME
func Dir() string {
	var path string
	if a := os.Getenv(KittyCADConfigDir); a != "" {
		path = a
	} else if b := os.Getenv(XDGConfigHome); b != "" {
		path = filepath.Join(b, KittyCADShorthandDir)
	} else if c := os.Getenv(AppData); runtime.GOOS == "windows" && c != "" {
		path = filepath.Join(c, KittyCADWindowsDir)
	} else {
		d, _ := os.UserHomeDir()
		path = filepath.Join(d, ".config", KittyCADShorthandDir)
	}

	// If the path does not exist and the KITTYCAD_CONFIG_DIR flag is not set try
	// migrating config from default paths.
	if !dirExists(path) && os.Getenv(KittyCADConfigDir) == "" {
		_ = autoMigrateConfigDir(path)
	}

	return path
}

// StateDir returns the path to the state directory.
// State path precedence
// 1. XDG_CONFIG_HOME
// 2. LocalAppData (windows only)
// 3. HOME
func StateDir() string {
	var path string
	if a := os.Getenv(XDGStateHome); a != "" {
		path = filepath.Join(a, KittyCADShorthandDir)
	} else if b := os.Getenv(LocalAppData); runtime.GOOS == "windows" && b != "" {
		path = filepath.Join(b, KittyCADWindowsDir)
	} else {
		c, _ := os.UserHomeDir()
		path = filepath.Join(c, ".local", "state", KittyCADShorthandDir)
	}

	// If the path does not exist try migrating state from default paths
	if !dirExists(path) {
		_ = autoMigrateStateDir(path)
	}

	return path
}

// DataDir returns the path to the data directory.
// Data path precedence
// 1. XDG_DATA_HOME
// 2. LocalAppData (windows only)
// 3. HOME
func DataDir() string {
	var path string
	if a := os.Getenv(XDGDataHome); a != "" {
		path = filepath.Join(a, KittyCADShorthandDir)
	} else if b := os.Getenv(LocalAppData); runtime.GOOS == "windows" && b != "" {
		path = filepath.Join(b, KittyCADWindowsDir)
	} else {
		c, _ := os.UserHomeDir()
		path = filepath.Join(c, ".local", "share", KittyCADShorthandDir)
	}

	return path
}

var errSamePath = errors.New("same path")
var errNotExist = errors.New("not exist")

// Check default path, os.UserHomeDir, for existing configs
// If configs exist then move them to newPath.
func autoMigrateConfigDir(newPath string) error {
	path, err := os.UserHomeDir()
	if oldPath := filepath.Join(path, ".config", KittyCADShorthandDir); err == nil && dirExists(oldPath) {
		return migrateDir(oldPath, newPath)
	}

	return errNotExist
}

// Check default path, os.UserHomeDir, for existing state file (state.yml)
// If state file exist then move it to newPath
func autoMigrateStateDir(newPath string) error {
	path, err := os.UserHomeDir()
	if oldPath := filepath.Join(path, ".config", KittyCADShorthandDir); err == nil && dirExists(oldPath) {
		return migrateFile(oldPath, newPath, "state.yml")
	}

	return errNotExist
}

func migrateFile(oldPath, newPath, file string) error {
	if oldPath == newPath {
		return errSamePath
	}

	oldFile := filepath.Join(oldPath, file)
	newFile := filepath.Join(newPath, file)

	if !fileExists(oldFile) {
		return errNotExist
	}

	_ = os.MkdirAll(filepath.Dir(newFile), 0755)
	return os.Rename(oldFile, newFile)
}

func migrateDir(oldPath, newPath string) error {
	if oldPath == newPath {
		return errSamePath
	}

	if !dirExists(oldPath) {
		return errNotExist
	}

	_ = os.MkdirAll(filepath.Dir(newPath), 0755)
	return os.Rename(oldPath, newPath)
}

func dirExists(path string) bool {
	f, err := os.Stat(path)
	return err == nil && f.IsDir()
}

func fileExists(path string) bool {
	f, err := os.Stat(path)
	return err == nil && !f.IsDir()
}

// File returns the path to the config file.
func File() string {
	return filepath.Join(Dir(), "config.yml")
}

// HostsConfigFile returns the path to the hosts config file.
func HostsConfigFile() string {
	return filepath.Join(Dir(), "hosts.yml")
}

// ParseDefaultConfig parses the default config file.
func ParseDefaultConfig() (Config, error) {
	return parseConfig(File())
}

// HomeDirPath returns the path to the home directory.
func HomeDirPath(subdir string) (string, error) {
	homeDir, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}

	newPath := filepath.Join(homeDir, subdir)
	return newPath, nil
}

// ReadConfigFile reads the config file.
var ReadConfigFile = func(filename string) ([]byte, error) {
	f, err := os.Open(filename)
	if err != nil {
		return nil, pathError(err)
	}
	defer f.Close()

	data, err := ioutil.ReadAll(f)
	if err != nil {
		return nil, err
	}

	return data, nil
}

// WriteConfigFile writes the config file.
var WriteConfigFile = func(filename string, data []byte) error {
	err := os.MkdirAll(filepath.Dir(filename), 0771)
	if err != nil {
		return pathError(err)
	}

	cfgFile, err := os.OpenFile(filename, os.O_RDWR|os.O_CREATE|os.O_TRUNC, 0600) // cargo coded from setup
	if err != nil {
		return err
	}
	defer cfgFile.Close()

	_, err = cfgFile.Write(data)
	return err
}

// BackupConfigFile backs up the config file.
var BackupConfigFile = func(filename string) error {
	return os.Rename(filename, filename+".bak")
}

func parseConfigFile(filename string) ([]byte, *yaml.Node, error) {
	data, err := ReadConfigFile(filename)
	if err != nil {
		return nil, nil, err
	}

	root, err := parseConfigData(data)
	if err != nil {
		return nil, nil, err
	}
	return data, root, err
}

func parseConfigData(data []byte) (*yaml.Node, error) {
	var root yaml.Node
	err := yaml.Unmarshal(data, &root)
	if err != nil {
		return nil, err
	}

	if len(root.Content) == 0 {
		return &yaml.Node{
			Kind:    yaml.DocumentNode,
			Content: []*yaml.Node{{Kind: yaml.MappingNode}},
		}, nil
	}
	if root.Content[0].Kind != yaml.MappingNode {
		return &root, fmt.Errorf("expected a top level map")
	}
	return &root, nil
}

func parseConfig(filename string) (Config, error) {
	_, root, err := parseConfigFile(filename)
	if err != nil {
		if os.IsNotExist(err) {
			root = NewBlankRoot()
		} else {
			return nil, err
		}
	}

	if _, hostsRoot, err := parseConfigFile(HostsConfigFile()); err == nil {
		if len(hostsRoot.Content[0].Content) > 0 {
			newContent := []*yaml.Node{
				{Value: "hosts"},
				hostsRoot.Content[0],
			}
			restContent := root.Content[0].Content
			root.Content[0].Content = append(newContent, restContent...)
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return nil, err
	}

	return NewConfig(root), nil
}

func pathError(err error) error {
	var pathError *os.PathError
	if errors.As(err, &pathError) && errors.Is(pathError.Err, syscall.ENOTDIR) {
		if p := findRegularFile(pathError.Path); p != "" {
			return fmt.Errorf("remove or rename regular file `%s` (must be a directory)", p)
		}

	}
	return err
}

func findRegularFile(p string) string {
	for {
		if s, err := os.Stat(p); err == nil && s.Mode().IsRegular() {
			return p
		}
		newPath := filepath.Dir(p)
		if newPath == p || newPath == "/" || newPath == "." {
			break
		}
		p = newPath
	}
	return ""
}

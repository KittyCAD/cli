package cmdutil

import (
	"errors"
	"fmt"
	"io"
	"io/ioutil"
	"os"
)

// ReadFile reads the file at the given path and returns the contents.
// If "-" is given, it reads from stdin.
func ReadFile(filename string, stdin io.ReadCloser) ([]byte, error) {
	if filename == "-" {
		b, err := ioutil.ReadAll(stdin)
		_ = stdin.Close()
		return b, err
	}

	if _, err := os.Stat(filename); errors.Is(err, os.ErrNotExist) {
		return nil, fmt.Errorf("file does not exist: %s", filename)
	}

	return ioutil.ReadFile(filename)
}

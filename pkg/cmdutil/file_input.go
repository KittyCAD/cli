package cmdutil

import (
	"io"
	"io/ioutil"
)

// ReadFile reads the file at the given path and returns the contents.
// If "-" is given, it reads from stdin.
func ReadFile(filename string, stdin io.ReadCloser) ([]byte, error) {
	if filename == "-" {
		b, err := ioutil.ReadAll(stdin)
		_ = stdin.Close()
		return b, err
	}

	return ioutil.ReadFile(filename)
}

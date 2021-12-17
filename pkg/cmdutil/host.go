package cmdutil

import (
	"errors"
	"strings"
)

// HostnameValidator is a function that validates a hostname.
func HostnameValidator(v interface{}) error {
	hostname, valid := v.(string)
	if !valid {
		return errors.New("hostname is not a string")
	}

	if len(strings.TrimSpace(hostname)) < 1 {
		return errors.New("a value is required")
	}
	// Allow for localhost, but other than that, require a valid domain.
	if strings.ContainsRune(hostname, '/') || (strings.ContainsRune(hostname, ':') && !strings.HasPrefix(hostname, "localhost:")) {
		return errors.New("invalid hostname")
	}
	return nil
}

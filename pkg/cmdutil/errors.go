package cmdutil

import (
	"errors"
	"fmt"

	"github.com/AlecAivazis/survey/v2/terminal"
)

// FlagErrorf returns a new FlagError that wraps an error produced by
// fmt.Errorf(format, args...).
func FlagErrorf(format string, args ...interface{}) error {
	return FlagErrorWrap(fmt.Errorf(format, args...))
}

// FlagErrorWrap returns a new FlagError that wraps the specified error.
func FlagErrorWrap(err error) error { return &FlagError{err} }

// FlagError indicates an error processing command-line flags or other arguments.
// Such errors cause the application to display the usage message.
type FlagError struct {
	// Note: not struct{error}: only *FlagError should satisfy error.
	err error
}

// Error implements the error interface.
func (fe *FlagError) Error() string {
	return fe.err.Error()
}

// Unwrap returns the underlying error.
func (fe *FlagError) Unwrap() error {
	return fe.err
}

// ErrSilent is an error that triggers exit code 1 without any error messaging.
var ErrSilent = errors.New("SilentError")

// ErrCancel signals user-initiated cancellation.
var ErrCancel = errors.New("CancelError")

// IsUserCancellation returns true if the user cancelled the operation.
func IsUserCancellation(err error) bool {
	return errors.Is(err, ErrCancel) || errors.Is(err, terminal.InterruptErr)
}

// MutuallyExclusive sets of flags that are mutually exclusive.
func MutuallyExclusive(message string, conditions ...bool) error {
	numTrue := 0
	for _, ok := range conditions {
		if ok {
			numTrue++
		}
	}
	if numTrue > 1 {
		return FlagErrorf("%s", message)
	}
	return nil
}

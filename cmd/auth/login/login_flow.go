package login

import (
	"context"
	"errors"
	"fmt"

	"github.com/AlecAivazis/survey/v2"
	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/cli/cli/v2/pkg/prompt"
	"github.com/kittycad/kittycad.go"
)

type iconfig interface {
	Get(string, string) (string, error)
	Set(string, string, string) error
	Write() error
}

// FlowOptions are options for the login flow.
type FlowOptions struct {
	IO             *iostreams.IOStreams
	Config         iconfig
	KittyCADClient func(string) (*kittycad.Client, error)
	Hostname       string
	Interactive    bool
	Web            bool
	Executable     string
	Context        context.Context
}

// Flow runs the login flow.
func Flow(opts *FlowOptions) error {
	cfg := opts.Config
	hostname := opts.Hostname
	cs := opts.IO.ColorScheme()

	fmt.Fprint(opts.IO.ErrOut, heredoc.Docf(`
			Tip: you can generate an API Token here https://%s/account
		`, hostname))

	var authToken string
	if err := prompt.SurveyAskOne(&survey.Password{
		Message: "Paste your authentication token:",
	}, &authToken, survey.WithValidator(survey.Required)); err != nil {
		return fmt.Errorf("could not prompt: %w", err)
	}

	if err := cfg.Set(hostname, "token", authToken); err != nil {
		return err
	}

	kittycadClient, err := opts.KittyCADClient(hostname)
	if err != nil {
		return err
	}

	// Get the session for the token.
	session, err := kittycadClient.MetaDebugSession()
	if err != nil {
		var httpErr kittycad.HTTPError
		if errors.As(err, &httpErr) && (httpErr.StatusCode >= 401 && httpErr.StatusCode < 500) {
			return fmt.Errorf("there was a problem with your token. The HTTP call returned `%d`. %s", httpErr.StatusCode, httpErr.Message)
		}
		return err
	}

	if err := cfg.Set(hostname, "user", session.Email); err != nil {
		return err
	}

	// Save the config.
	if err := cfg.Write(); err != nil {
		return err
	}

	fmt.Fprintf(opts.IO.ErrOut, "%s Logged in as %s\n", cs.SuccessIcon(), cs.Bold(session.Email))
	return nil
}

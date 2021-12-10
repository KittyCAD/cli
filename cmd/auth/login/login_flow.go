package login

import (
	"context"
	"fmt"

	"github.com/AlecAivazis/survey/v2"
	"github.com/MakeNowJust/heredoc"
	"github.com/cli/cli/v2/pkg/iostreams"
	"github.com/cli/cli/v2/pkg/prompt"
	"github.com/kittycad/cli/kittycad"
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
	KittyCADClient func() (*kittycad.Client, error)
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

	kittycadClient, err := opts.KittyCADClient()
	if err != nil {
		return err
	}

	// Get the session for the token.
	session, err := kittycadClient.MetaDebugSession(opts.Context)
	if err != nil {
		// TODO: do a better error message here like we did in main.go
		return err
	}

	// TODO: return the user's email instead.
	fmt.Fprintf(opts.IO.ErrOut, "%s Logged in as %s\n", cs.SuccessIcon(), cs.Bold(*session.UserId))
	return nil
}

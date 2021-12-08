package main

import (
	"fmt"

	"github.com/urfave/cli/v2"
)

func metaSession(c *cli.Context) error {
	session, err := kittycadClient.MetaDebugSessionWithResponse(c.Context)
	if err != nil {
		return fmt.Errorf("failed to get auth session: %w", err)
	}

	fmt.Printf("%s\n", session.Body)
	return nil
}

func metaInstance(c *cli.Context) error {
	instance, err := kittycadClient.MetaDebugInstanceWithResponse(c.Context)
	if err != nil {
		return fmt.Errorf("failed to get auth instance: %w", err)
	}

	fmt.Printf("%s\n", instance.Body)
	return nil
}

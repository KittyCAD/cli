package main

import (
	"fmt"

	"github.com/urfave/cli/v2"
)

func metaSession(c *cli.Context) error {
	session, err := kittycadClient.MetaDebugSession(c.Context)
	if err != nil {
		return fmt.Errorf("failed to get auth session: %w", err)
	}

	fmt.Printf("%#v\n", session)
	return nil
}

func metaInstance(c *cli.Context) error {
	instance, err := kittycadClient.MetaDebugInstance(c.Context)
	if err != nil {
		return fmt.Errorf("failed to get auth instance: %w", err)
	}

	fmt.Printf("%#v\n", instance)
	return nil
}

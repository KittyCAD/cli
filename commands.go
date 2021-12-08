package main

import (
	"fmt"

	"github.com/kittycad/kittycad.go"
	"github.com/urfave/cli/v2"
)

func metaSession(c *cli.Context) error {
	resp, err := kittycadClient.MetaDebugSession(c.Context)
	if err != nil {
		return fmt.Errorf("failed to get auth session: %w", err)
	}

	session, err := kittycad.ParseMetaDebugSessionResponse(resp)
	if err != nil {
		return fmt.Errorf("failed to parse auth session response: %w", err)
	}

	fmt.Printf("%s\n", session.Body)
	return nil
}

func metaInstance(c *cli.Context) error {
	resp, err := kittycadClient.MetaDebugInstance(c.Context)
	if err != nil {
		return fmt.Errorf("failed to get auth instance: %w", err)
	}

	instance, err := kittycad.ParseMetaDebugInstanceResponse(resp)
	if err != nil {
		return fmt.Errorf("failed to parse auth instance response: %w", err)
	}

	fmt.Printf("%s\n", instance.Body)
	return nil
}

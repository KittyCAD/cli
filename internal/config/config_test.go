package config

import (
	"bytes"
	"testing"

	"github.com/MakeNowJust/heredoc"
	"github.com/stretchr/testify/assert"
)

func Test_fileConfig_Set(t *testing.T) {
	mainBuf := bytes.Buffer{}
	hostsBuf := bytes.Buffer{}
	defer StubWriteConfig(&mainBuf, &hostsBuf)()

	c := NewBlankConfig()
	assert.NoError(t, c.Set("", "prompt", "disable"))
	assert.NoError(t, c.Set("api.kittycad.io", "pager", "cat"))
	assert.NoError(t, c.Set("not.kittycad.io", "pager", "less"))
	assert.NoError(t, c.Set("api.kittycad.io", "token", "BLAH"))
	assert.NoError(t, c.Write())

	assert.Contains(t, mainBuf.String(), "prompt: disable")
	assert.Equal(t, `api.kittycad.io:
    pager: cat
    token: BLAH
not.kittycad.io:
    pager: less
`, hostsBuf.String())
}

func Test_defaultConfig(t *testing.T) {
	mainBuf := bytes.Buffer{}
	hostsBuf := bytes.Buffer{}
	defer StubWriteConfig(&mainBuf, &hostsBuf)()

	cfg := NewBlankConfig()
	assert.NoError(t, cfg.Write())

	expected := heredoc.Doc(`
		# When to interactively prompt. This is a global config that cannot be overridden by hostname. Supported values: enabled, disabled
		prompt: enabled
		# A pager program to send command output to, e.g. "less". Set the value to "cat" to disable the pager.
		pager:
		# Aliases allow you to create nicknames for kittycad commands
		aliases:
		    co: file convert
		# What web browser kittycad should use when opening URLs. If blank, will refer to environment.
		browser:
	`)
	assert.Equal(t, expected, mainBuf.String())
	assert.Equal(t, "", hostsBuf.String())

	aliases, err := cfg.Aliases()
	assert.NoError(t, err)
	assert.Equal(t, len(aliases.All()), 1)
	expansion, _ := aliases.Get("co")
	assert.Equal(t, expansion, "pr checkout")

	browser, err := cfg.Get("", "browser")
	assert.NoError(t, err)
	assert.Equal(t, "", browser)
}

func Test_ValidateValue(t *testing.T) {
	err = ValidateValue("got", "123")
	assert.NoError(t, err)
}

func Test_ValidateKey(t *testing.T) {
	err := ValidateKey("invalid")
	assert.EqualError(t, err, "invalid key")

	err = ValidateKey("prompt")
	assert.NoError(t, err)

	err = ValidateKey("pager")
	assert.NoError(t, err)

	err = ValidateKey("browser")
	assert.NoError(t, err)
}

package kittycad

import (
	"net/url"
	"testing"
)

type expandTest struct {
	in         string
	expansions map[string]string
	want       string
}

var expandTests = []expandTest{
	// no expansions
	{
		"",
		map[string]string{},
		DefaultServerURL,
	},
	// multiple expansions, no escaping
	{
		"file/convert/{{.srcFormat}}/{{.outputFormat}}",
		map[string]string{
			"srcFormat":    "step",
			"outputFormat": "obj",
		},
		DefaultServerURL + "/file/convert/step/obj",
	},
}

func TestExpandURL(t *testing.T) {
	for i, test := range expandTests {
		uri := resolveRelative(DefaultServerURL, test.in)
		u, err := url.Parse(uri)
		if err != nil {
			t.Fatalf("parsing url %q failed: %v", test.in, err)
		}
		expandURL(u, test.expansions)
		got := u.String()
		if got != test.want {
			t.Errorf("got %q expected %q in test %d", got, test.want, i+1)
		}
	}
}

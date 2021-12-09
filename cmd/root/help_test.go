package root

import (
	"testing"
)

func TestDedent(t *testing.T) {
	type c struct {
		input    string
		expected string
	}

	cases := []c{
		{
			input:    "      --help      Show help for command\n      --version   Show kittycad version\n",
			expected: "--help      Show help for command\n--version   Show kittycad version\n",
		},
		{
			input:    "  line 1\n\n  line 2\n line 3",
			expected: " line 1\n\n line 2\nline 3",
		},
		{
			input:    "  line 1\n  line 2\n  line 3\n\n",
			expected: "line 1\nline 2\nline 3\n\n",
		},
		{
			input:    "\n\n\n\n\n\n",
			expected: "\n\n\n\n\n\n",
		},
		{
			input:    "",
			expected: "",
		},
	}

	for _, tt := range cases {
		got := dedent(tt.input)
		if got != tt.expected {
			t.Errorf("expected: %q, got: %q", tt.expected, got)
		}
	}
}

package main

import (
	"io/ioutil"
	"strings"
	"testing"
)

func Test_run(t *testing.T) {
	dir := t.TempDir()
	args := []string{"--man-page", "--website", "--doc-path", dir}
	err := run(args)
	if err != nil {
		t.Fatalf("got error: %v", err)
	}

	manPage, err := ioutil.ReadFile(dir + "/kittycad-file-convert.1")
	if err != nil {
		t.Fatalf("error reading `kittycad-file-convert.1`: %v", err)
	}
	if !strings.Contains(string(manPage), `\fBkittycad file convert`) {
		t.Fatal("man page corrupted")
	}

	markdownPage, err := ioutil.ReadFile(dir + "/kittycad_file_convert.md")
	if err != nil {
		t.Fatalf("error reading `kittycad_file_convert.md`: %v", err)
	}
	if !strings.Contains(string(markdownPage), `## kittycad file convert`) {
		t.Fatal("markdown page corrupted")
	}
}

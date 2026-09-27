package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	noise "github.com/valsteen/noise-oxydation/go"
)

func TestReplayRejectsInputOutputAliases(t *testing.T) {
	dir := t.TempDir()
	input := filepath.Join(dir, "input.mulaw")
	contents := bytes.Repeat([]byte{0xff}, noise.PacketBytes)
	if err := os.WriteFile(input, contents, 0o600); err != nil {
		t.Fatal(err)
	}
	aliases := []string{input, filepath.Join(dir, "hardlink.mulaw"), filepath.Join(dir, "symlink.mulaw")}
	if err := os.Link(input, aliases[1]); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(input, aliases[2]); err != nil {
		t.Fatal(err)
	}
	for _, output := range aliases {
		if err := run(input, output, noise.SppMmse); err == nil || !strings.Contains(err.Error(), "input and output identify the same file") {
			t.Errorf("replay with input alias %q returned %v", output, err)
		}
		got, err := os.ReadFile(input)
		if err != nil {
			t.Fatal(err)
		}
		if !bytes.Equal(got, contents) {
			t.Fatalf("replay changed input through %q", output)
		}
	}
}

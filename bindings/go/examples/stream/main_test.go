package main

import (
	"bytes"
	"io"
	"testing"

	noiseoxydation "github.com/valsteen/noise-oxydation/bindings/go"
)

type shortWriter struct {
	bytes.Buffer
	maximum int
}

func (writer *shortWriter) Write(value []byte) (int, error) {
	if len(value) > writer.maximum {
		value = value[:writer.maximum]
	}
	return writer.Buffer.Write(value)
}

func TestStreamWritesOrderedOutputAndTrimsPadding(t *testing.T) {
	input := make([]byte, noiseoxydation.PacketBytes+37)
	for index := range input {
		input[index] = byte(index * 29)
	}
	writer := &shortWriter{maximum: 13}
	if err := stream(bytes.NewReader(input), writer); err != nil {
		t.Fatalf("stream: %v", err)
	}
	if got, want := writer.Len(), len(input); got != want {
		t.Fatalf("output length = %d; want input length %d", got, want)
	}

	processor := noiseoxydation.New()
	defer processor.Close()
	var packet [noiseoxydation.PacketBytes]byte
	var output [noiseoxydation.MaxPacketOutputBytes]byte
	var expected bytes.Buffer
	for offset := 0; offset < len(input); offset += len(packet) {
		for index := range packet {
			packet[index] = 0xff
		}
		end := min(offset+len(packet), len(input))
		copy(packet[:], input[offset:end])
		written, err := processor.Push(packet[:], output[:])
		if err != nil {
			t.Fatalf("push: %v", err)
		}
		_, _ = expected.Write(output[:written])
	}
	written, err := processor.Drain(output[:noiseoxydation.MaxDrainOutputBytes])
	if err != nil {
		t.Fatalf("drain: %v", err)
	}
	_, _ = expected.Write(output[:written])
	want := expected.Bytes()[:len(input)]
	if !bytes.Equal(writer.Bytes(), want) {
		t.Fatal("stream output differs from ordered Push output followed by one Drain")
	}
}

func TestStreamPropagatesReadAndWriterFailures(t *testing.T) {
	if err := stream(errorReader{}, io.Discard); err == nil {
		t.Fatal("stream accepted reader failure")
	}
	if err := stream(bytes.NewReader(make([]byte, noiseoxydation.PacketBytes)), errorWriter{}); err == nil {
		t.Fatal("stream accepted writer failure")
	}
}

type errorReader struct{}

func (errorReader) Read([]byte) (int, error) { return 0, io.ErrClosedPipe }

type errorWriter struct{}

func (errorWriter) Write([]byte) (int, error) { return 0, io.ErrClosedPipe }

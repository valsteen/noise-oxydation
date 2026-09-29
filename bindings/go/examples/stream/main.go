package main

import (
	"errors"
	"fmt"
	"io"
	"os"

	noiseoxydation "github.com/valsteen/noise-oxydation/bindings/go"
)

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintln(os.Stderr, "usage: stream <input.ulaw>")
		os.Exit(2)
	}
	input, err := os.Open(os.Args[1])
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	defer input.Close()
	if err := stream(input, os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func stream(source io.Reader, destination io.Writer) error {
	processor := noiseoxydation.New()
	defer processor.Close()
	var packet [noiseoxydation.PacketBytes]byte
	var output [noiseoxydation.MaxPacketOutputBytes]byte
	remainingOutput := 0
	for {
		read, readErr := io.ReadFull(source, packet[:])
		if read == 0 && readErr != nil {
			if errors.Is(readErr, io.EOF) {
				break
			}
			return readErr
		}
		remainingOutput += read
		for index := read; index < len(packet); index++ {
			packet[index] = 0xff
		}
		written, err := processor.Push(packet[:], output[:])
		if err != nil {
			return err
		}
		if err := writeValid(destination, output[:written], &remainingOutput); err != nil {
			return err
		}
		if readErr != nil {
			if !errors.Is(readErr, io.ErrUnexpectedEOF) && !errors.Is(readErr, io.EOF) {
				if err := drainRemaining(processor, destination, &output, &remainingOutput); err != nil {
					return err
				}
				return readErr
			}
			break
		}
	}
	if err := drainRemaining(processor, destination, &output, &remainingOutput); err != nil {
		return err
	}
	if remainingOutput != 0 {
		return fmt.Errorf("stream ended with %d unwritten output bytes", remainingOutput)
	}
	return nil
}

func drainRemaining(processor *noiseoxydation.Processor, destination io.Writer, output *[noiseoxydation.MaxPacketOutputBytes]byte, remaining *int) error {
	written, err := processor.Drain(output[:noiseoxydation.MaxDrainOutputBytes])
	if err != nil {
		return err
	}
	return writeValid(destination, output[:written], remaining)
}

func writeValid(destination io.Writer, output []byte, remaining *int) error {
	if len(output) > *remaining {
		output = output[:*remaining]
	}
	for len(output) > 0 {
		written, err := destination.Write(output)
		if written < 0 || written > len(output) {
			return fmt.Errorf("writer returned invalid count %d", written)
		}
		*remaining -= written
		output = output[written:]
		if err != nil {
			return err
		}
		if written == 0 {
			return io.ErrShortWrite
		}
	}
	return nil
}

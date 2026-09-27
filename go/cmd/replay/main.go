// Replay a prepared, packet-aligned 8 kHz mono G.711 μ-law call.
package main

import (
	"bufio"
	"flag"
	"fmt"
	"io"
	"os"

	noise "github.com/valsteen/noise-oxydation/go"
)

func writeBatch(w *bufio.Writer, output *[noise.OutputBytes]byte, batch noise.Batch) (int, error) {
	valid := 0
	for i := 0; i < batch.Count; i++ {
		n := noise.PacketBytes
		if i+1 == batch.Count {
			n = batch.FinalValid
		}
		start := i * noise.PacketBytes
		if _, err := w.Write(output[start : start+n]); err != nil {
			return valid, err
		}
		valid += n
	}
	return valid, nil
}

func run(inputPath, outputPath string, estimator noise.Estimator, mode noise.ProcessingMode) (err error) {
	in, err := os.Open(inputPath)
	if err != nil {
		return err
	}
	defer in.Close()
	inputInfo, err := in.Stat()
	if err != nil {
		return err
	}
	outputInfo, statErr := os.Stat(outputPath)
	if statErr == nil {
		if os.SameFile(inputInfo, outputInfo) {
			return fmt.Errorf("input and output identify the same file: %q", outputPath)
		}
	} else if !os.IsNotExist(statErr) {
		return statErr
	}
	out, err := os.Create(outputPath)
	if err != nil {
		return err
	}
	defer func() {
		if closeErr := out.Close(); err == nil {
			err = closeErr
		}
	}()
	call, err := noise.New(noise.Config{LearningDuration: noise.DefaultConfig().LearningDuration, Estimator: estimator, Mode: mode})
	if err != nil {
		return err
	}
	defer call.Close()
	reader := bufio.NewReader(in)
	writer := bufio.NewWriter(out)
	var packet [noise.PacketBytes]byte
	var output [noise.OutputBytes]byte
	inputBytes, outputBytes := 0, 0
	for {
		n, readErr := io.ReadFull(reader, packet[:])
		if readErr == io.EOF {
			break
		}
		if readErr != nil {
			return fmt.Errorf("input must contain whole 160-byte packets (last read %d): %w", n, readErr)
		}
		inputBytes += n
		batch, processErr := call.Process(&packet, &output)
		if processErr != nil {
			return processErr
		}
		n, err = writeBatch(writer, &output, batch)
		if err != nil {
			return err
		}
		outputBytes += n
	}
	batch, err := call.Finish(&output)
	if err != nil {
		return err
	}
	n, err := writeBatch(writer, &output, batch)
	if err != nil {
		return err
	}
	outputBytes += n
	if outputBytes != inputBytes {
		return fmt.Errorf("valid output bytes %d, input bytes %d", outputBytes, inputBytes)
	}
	if err := writer.Flush(); err != nil {
		return err
	}
	fmt.Printf("processed %d valid bytes\n", outputBytes)
	return nil
}

func main() {
	input := flag.String("input", "", "packet-aligned 8 kHz mono μ-law input")
	output := flag.String("output", "", "μ-law output")
	estimator := flag.String("estimator", "spp", "spp, mcra, or minimum")
	mode := flag.String("mode", "conservative", "conservative or experimental")
	flag.Parse()
	choices := map[string]noise.Estimator{"spp": noise.SppMmse, "mcra": noise.Mcra, "minimum": noise.Minimum}
	selected, ok := choices[*estimator]
	modes := map[string]noise.ProcessingMode{"conservative": noise.Conservative, "experimental": noise.ExperimentalLowDelay}
	selectedMode, modeOK := modes[*mode]
	if *input == "" || *output == "" || !ok || !modeOK {
		fmt.Fprintln(os.Stderr, "usage: replay -input prepared.mulaw -output enhanced.mulaw [-estimator spp|mcra|minimum] [-mode conservative|experimental]")
		os.Exit(2)
	}
	if err := run(*input, *output, selected, selectedMode); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

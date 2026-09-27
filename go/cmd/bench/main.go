// Bench measures the Go/cgo packet path over a prepared full call, excluding I/O.
package main

import (
	"flag"
	"fmt"
	"os"
	"runtime"
	"slices"
	"time"

	noise "github.com/valsteen/noise-oxydation/go"
)

func percentile(sorted []time.Duration, percent int) time.Duration {
	index := (len(sorted)*percent+99)/100 - 1
	return sorted[index]
}

func main() {
	inputPath := flag.String("input", "", "packet-aligned representative μ-law call")
	flag.Parse()
	input, err := os.ReadFile(*inputPath)
	if err != nil || len(input) == 0 || len(input)%noise.PacketBytes != 0 {
		fmt.Fprintln(os.Stderr, "input must be a nonempty packet-aligned μ-law file:", err)
		os.Exit(2)
	}
	packets := len(input) / noise.PacketBytes
	var packet [noise.PacketBytes]byte
	var output [noise.OutputBytes]byte
	for _, estimator := range []noise.Estimator{noise.SppMmse, noise.Mcra, noise.Minimum} {
		call, err := noise.New(noise.Config{LearningDuration: 5 * time.Second, Estimator: estimator})
		if err != nil {
			panic(err)
		}
		for i := 0; i < packets; i++ {
			copy(packet[:], input[i*noise.PacketBytes:(i+1)*noise.PacketBytes])
			if _, err := call.Process(&packet, &output); err != nil {
				panic(err)
			}
		}
		if _, err := call.Finish(&output); err != nil {
			panic(err)
		}
		if err := call.Reset(); err != nil {
			panic(err)
		}
		latencies := make([]time.Duration, packets)
		runtime.GC()
		var before, after runtime.MemStats
		runtime.ReadMemStats(&before)
		valid := 0
		for i := 0; i < packets; i++ {
			copy(packet[:], input[i*noise.PacketBytes:(i+1)*noise.PacketBytes])
			start := time.Now()
			batch, err := call.Process(&packet, &output)
			latencies[i] = time.Since(start)
			if err != nil {
				panic(err)
			}
			valid += batch.Count * noise.PacketBytes
			if batch.Count > 0 {
				valid -= noise.PacketBytes - batch.FinalValid
			}
		}
		batch, err := call.Finish(&output)
		if err != nil {
			panic(err)
		}
		valid += batch.Count * noise.PacketBytes
		if batch.Count > 0 {
			valid -= noise.PacketBytes - batch.FinalValid
		}
		runtime.ReadMemStats(&after)
		if valid != len(input) {
			panic("output length mismatch")
		}
		if err := call.Close(); err != nil {
			panic(err)
		}
		slices.Sort(latencies)
		fmt.Printf("estimator=%d packets=%d p50=%s p95=%s go_allocs_total=%d go_allocs_per_packet=%.3f valid_bytes=%d\n", estimator, packets, percentile(latencies, 50), percentile(latencies, 95), after.Mallocs-before.Mallocs, float64(after.Mallocs-before.Mallocs)/float64(packets), valid)
	}
}

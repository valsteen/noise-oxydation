package main

import (
	"crypto/sha256"
	"errors"
	"flag"
	"fmt"
	"math"
	"os"
	"runtime"
	"runtime/debug"
	"slices"
	"time"

	noiseoxydation "github.com/valsteen/noise-oxydation/bindings/go"
)

const (
	quietIntroBytes = 40_000
	packetDuration  = 20 * time.Millisecond
)

func main() {
	flag.Usage = func() {
		fmt.Fprintln(flag.CommandLine.Output(), "usage: measure <representative-input.ulaw>")
		flag.PrintDefaults()
	}
	flag.Parse()
	if flag.NArg() != 1 {
		flag.Usage()
		os.Exit(2)
	}
	sample, err := os.ReadFile(flag.Arg(0))
	if err != nil {
		fail(err)
	}
	if len(sample) == 0 {
		fail(errors.New("measurement input is empty"))
	}
	packets := (quietIntroBytes + len(sample) + noiseoxydation.PacketBytes - 1) / noiseoxydation.PacketBytes
	if packets == 0 {
		fail(errors.New("measurement input contains no packets"))
	}
	allocationCount, allocatedBytes, err := allocationPass(sample, packets)
	if err != nil {
		fail(err)
	}
	latencies, drainLatency, err := timedPass(sample, packets)
	if err != nil {
		fail(err)
	}
	slices.Sort(latencies)
	sampleHash := sha256.Sum256(sample)
	milliseconds := func(duration time.Duration) float64 { return float64(duration) / float64(time.Millisecond) }
	fmt.Printf("Host: %s/%s (%s)\n", runtime.GOOS, runtime.GOARCH, runtime.Version())
	if buildInfo, ok := debug.ReadBuildInfo(); ok {
		fmt.Printf("Go module: %s\n", buildInfo.Main.Path)
	}
	fmt.Printf("Input: %s (%d μ-law bytes, SHA-256 %x)\n", flag.Arg(0), len(sample), sampleHash)
	fmt.Printf("Packets: %d (including %d-byte quiet calibration prefix and final silence padding)\n", packets, quietIntroBytes)
	fmt.Printf("Go allocations per packet: %.4f (%.2f bytes/packet)\n", float64(allocationCount)/float64(packets), float64(allocatedBytes)/float64(packets))
	fmt.Printf("Push latency: p50 %.3f ms, p95 %.3f ms, p99 %.3f ms, max %.3f ms\n", milliseconds(percentile(latencies, 0.50)), milliseconds(percentile(latencies, 0.95)), milliseconds(percentile(latencies, 0.99)), milliseconds(latencies[len(latencies)-1]))
	fmt.Printf("Drain latency: %.3f ms\n", milliseconds(drainLatency))
	fmt.Printf("20 ms packet cadence: p99 %.1f%%, max %.1f%%\n", ratio(percentile(latencies, 0.99), packetDuration), ratio(latencies[len(latencies)-1], packetDuration))
	fmt.Println("One-host call timing is evidence for this input and environment; it does not guarantee scheduling, end-to-end latency, or speech quality.")
}

func allocationPass(sample []byte, packetCount int) (uint64, uint64, error) {
	processor := noiseoxydation.New()
	defer processor.Close()
	var packet [noiseoxydation.PacketBytes]byte
	var output [noiseoxydation.MaxPacketOutputBytes]byte
	var tail [noiseoxydation.MaxDrainOutputBytes]byte
	runtime.GC()
	var before runtime.MemStats
	runtime.ReadMemStats(&before)
	for index := 0; index < packetCount; index++ {
		fillPacket(sample, index, &packet)
		if _, err := processor.Push(packet[:], output[:]); err != nil {
			return 0, 0, err
		}
	}
	if _, err := processor.Drain(tail[:]); err != nil {
		return 0, 0, err
	}
	var after runtime.MemStats
	runtime.ReadMemStats(&after)
	return after.Mallocs - before.Mallocs, after.TotalAlloc - before.TotalAlloc, nil
}

func timedPass(sample []byte, packetCount int) ([]time.Duration, time.Duration, error) {
	processor := noiseoxydation.New()
	defer processor.Close()
	var packet [noiseoxydation.PacketBytes]byte
	var output [noiseoxydation.MaxPacketOutputBytes]byte
	var tail [noiseoxydation.MaxDrainOutputBytes]byte
	latencies := make([]time.Duration, packetCount)
	for index := range latencies {
		fillPacket(sample, index, &packet)
		started := time.Now()
		if _, err := processor.Push(packet[:], output[:]); err != nil {
			return nil, 0, err
		}
		latencies[index] = time.Since(started)
	}
	started := time.Now()
	if _, err := processor.Drain(tail[:]); err != nil {
		return nil, 0, err
	}
	return latencies, time.Since(started), nil
}

func fillPacket(sample []byte, packetIndex int, packet *[noiseoxydation.PacketBytes]byte) {
	for index := range packet {
		packet[index] = 0xff
	}
	packetStart := packetIndex * noiseoxydation.PacketBytes
	packetEnd := packetStart + noiseoxydation.PacketBytes
	sampleStart := max(packetStart, quietIntroBytes) - quietIntroBytes
	sampleEnd := min(packetEnd, quietIntroBytes+len(sample)) - quietIntroBytes
	if sampleStart >= sampleEnd {
		return
	}
	packetOffset := max(packetStart, quietIntroBytes) - packetStart
	copy(packet[packetOffset:packetOffset+sampleEnd-sampleStart], sample[sampleStart:sampleEnd])
}

func percentile(sorted []time.Duration, quantile float64) time.Duration {
	index := int(math.Ceil(quantile*float64(len(sorted)))) - 1
	return sorted[max(0, index)]
}

func ratio(value, limit time.Duration) float64 { return float64(value) * 100 / float64(limit) }

func fail(err error) {
	fmt.Fprintln(os.Stderr, err)
	os.Exit(1)
}

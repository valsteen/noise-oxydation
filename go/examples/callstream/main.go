// Command callstream streams one call's G.711 mu-law packets through the noiseox enhancer the way a call-handling
// service would, and reports packet counts, per-packet latency and heap allocations of the call loop.
//
// The loop reads each 160-byte packet into one reused input array, processes it into one reused output array, writes
// every emitted packet in order to a buffered writer, drains the withheld packets at the end and closes the call.
// Nothing in the loop allocates; the report shows the heap allocation count measured around it.
//
//	cargo build --locked --release -p noise-oxydation-capi          # from the repository root
//	go run ./examples/callstream -in call.ul -out enhanced.ul       # from go/
//	go run ./examples/callstream -synthetic 3000                     # generated input, output discarded
//	go run ./examples/callstream -in call.ul -pace                   # real-time pacing, one packet per 20 ms
//	go run ./examples/callstream -in call.ul -passes 20 -parallel 100
//
// -passes repeats the stream on the same Call with Reset between passes for more latency samples (output is written
// for the first pass only). -parallel N then runs N independent calls in parallel goroutines, one Call each, outside
// the measured loop, and reports their wall time and real-time factor. The input is a headerless 8 kHz mu-law file,
// or standard input with -in -; a final partial packet is padded with mu-law silence (0xFF).
//
// The command exits with status 1 if the call's output packet count differs from its input packet count, if the call
// loop allocated, or if the parallel calls disagree with each other. The allocation count is process-wide, so with
// -pace it also counts the Go runtime's own background allocations while the loop waits for the ticker; paced
// runs report the count without failing on it.
package main

import (
	"bufio"
	"bytes"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"runtime"
	"slices"
	"sync"
	"time"

	noiseox "github.com/valsteen/noise-oxydation-claude/go"
)

// silence is the mu-law code of a zero sample.
const silence = 0xFF

// cadence is the duration of one packet.
const cadence = 20 * time.Millisecond

type options struct {
	in, out         string
	synthetic       int
	pace            bool
	estimator       string
	interference    bool
	passes          int
	parallel        int
	parallelSeconds int
}

func main() {
	var opts options
	flag.StringVar(&opts.in, "in", "", "headerless 8 kHz mu-law input file, or - for standard input")
	flag.StringVar(&opts.out, "out", "", "enhanced mu-law output file, - for standard output, empty to discard")
	flag.IntVar(&opts.synthetic, "synthetic", 0, "generate this many packets of synthetic call audio instead of -in")
	flag.BoolVar(&opts.pace, "pace", false, "send one packet every 20 ms, as a live call does")
	flag.StringVar(&opts.estimator, "estimator", "spp-mmse", "noise estimator: spp-mmse, mcra or minimum")
	flag.BoolVar(&opts.interference, "interference", true, "enable tonal transient suppression")
	flag.IntVar(&opts.passes, "passes", 1, "stream the input this many times on the same call, with Reset between")
	flag.IntVar(&opts.parallel, "parallel", 0, "afterwards, run this many independent calls in parallel goroutines")
	flag.IntVar(&opts.parallelSeconds, "parallel-seconds", 120, "audio seconds per parallel call, looping the input")
	flag.Parse()
	if err := run(opts); err != nil {
		fmt.Fprintln(os.Stderr, "callstream:", err)
		os.Exit(1)
	}
}

func config(opts options) (noiseox.Config, error) {
	config := noiseox.DefaultConfig()
	switch opts.estimator {
	case "spp-mmse":
		config.NoiseEstimator = noiseox.NoiseEstimatorSppMmse
	case "mcra":
		config.NoiseEstimator = noiseox.NoiseEstimatorMcra
	case "minimum":
		config.NoiseEstimator = noiseox.NoiseEstimatorMinimum
	default:
		return config, fmt.Errorf("unknown estimator %q", opts.estimator)
	}
	if !opts.interference {
		config.Interference = noiseox.InterferenceDisabled
	}
	return config, nil
}

// source opens the input. Files and synthetic input can be rewound for further passes and read again for the
// parallel calls; standard input can only be streamed once.
func source(opts options) (io.Reader, func() ([]byte, error), error) {
	switch {
	case opts.synthetic > 0 && opts.in != "":
		return nil, nil, errors.New("use either -in or -synthetic")
	case opts.synthetic > 0:
		data := synthetic(opts.synthetic)
		return bytes.NewReader(data), func() ([]byte, error) { return data, nil }, nil
	case opts.in == "-":
		return os.Stdin, nil, nil
	case opts.in != "":
		file, err := os.Open(opts.in)
		if err != nil {
			return nil, nil, err
		}
		return file, func() ([]byte, error) { return os.ReadFile(opts.in) }, nil
	default:
		return nil, nil, errors.New("give -in FILE, -in - or -synthetic N")
	}
}

func run(opts options) error {
	callConfig, err := config(opts)
	if err != nil {
		return err
	}
	input, readAll, err := source(opts)
	if err != nil {
		return err
	}
	if closer, ok := input.(io.Closer); ok && input != os.Stdin {
		defer closer.Close()
	}
	if opts.passes < 1 {
		return errors.New("-passes must be at least 1")
	}
	seeker, seekable := input.(io.Seeker)
	if opts.passes > 1 && !seekable {
		return errors.New("-passes needs a file or synthetic input")
	}
	if opts.parallel > 0 && readAll == nil {
		return errors.New("-parallel needs a file or synthetic input")
	}

	output := io.Discard
	switch opts.out {
	case "":
	case "-":
		output = os.Stdout
	default:
		file, err := os.Create(opts.out)
		if err != nil {
			return err
		}
		defer file.Close()
		output = file
	}

	call, err := noiseox.New(callConfig)
	if err != nil {
		return err
	}
	defer call.Close()

	// Everything the loop needs is allocated here: the reader and writer buffers, the packet arrays, the drain
	// buffer, the latency samples and the pacing ticker.
	reader := bufio.NewReaderSize(input, 64*1024)
	writer := bufio.NewWriterSize(output, 64*1024)
	var in, out [noiseox.PacketSize]byte
	var tail [noiseox.DelayPackets][noiseox.PacketSize]byte
	latencies := make([]time.Duration, 0, latencyCapacity(opts, input))
	var ticker *time.Ticker
	if opts.pace {
		ticker = time.NewTicker(cadence)
		defer ticker.Stop()
	}

	var stats loopStats
	var before, after runtime.MemStats
	runtime.GC()
	runtime.ReadMemStats(&before)
	for pass := range opts.passes {
		if pass > 0 {
			if _, err := seeker.Seek(0, io.SeekStart); err != nil {
				return err
			}
			reader.Reset(input)
			if err := call.Reset(); err != nil {
				return err
			}
		}
		// Output is written for the first pass only; later passes only add latency samples.
		if err := stream(call, reader, writer, pass == 0, ticker, &in, &out, &tail, &latencies, &stats); err != nil {
			return err
		}
	}
	runtime.ReadMemStats(&after)
	if err := writer.Flush(); err != nil {
		return err
	}
	if err := call.Close(); err != nil {
		return err
	}

	report(opts, &stats, latencies, after.Mallocs-before.Mallocs, after.TotalAlloc-before.TotalAlloc)
	if stats.packetsOut != stats.packetsIn {
		return fmt.Errorf("the call emitted %d packets for %d input packets", stats.packetsOut, stats.packetsIn)
	}
	if after.Mallocs != before.Mallocs && !opts.pace {
		return fmt.Errorf("the call loop allocated %d times", after.Mallocs-before.Mallocs)
	}
	if opts.parallel > 0 {
		data, err := readAll()
		if err != nil {
			return err
		}
		return runParallel(callConfig, data, opts.parallel, opts.parallelSeconds)
	}
	return nil
}

type loopStats struct {
	// packetsIn and packetsOut count the first pass, whose output is written.
	packetsIn, packetsOut int
	padded                int
}

// stream runs one call from the first packet to the drain. It performs no allocation.
func stream(
	call *noiseox.Call, reader *bufio.Reader, writer *bufio.Writer, write bool, ticker *time.Ticker,
	in, out *[noiseox.PacketSize]byte, tail *[noiseox.DelayPackets][noiseox.PacketSize]byte,
	latencies *[]time.Duration, stats *loopStats,
) error {
	for {
		n, err := io.ReadFull(reader, in[:])
		if n == 0 && (err == io.EOF || err == io.ErrUnexpectedEOF) {
			break
		}
		if err == io.ErrUnexpectedEOF {
			// A final partial packet is padded with silence.
			for i := n; i < len(in); i++ {
				in[i] = silence
			}
			if write {
				stats.padded = len(in) - n
			}
		} else if err != nil {
			return err
		}
		if ticker != nil {
			<-ticker.C
		}
		start := time.Now()
		emitted, err := call.ProcessPacket(in, out)
		elapsed := time.Since(start)
		if err != nil {
			return err
		}
		if len(*latencies) < cap(*latencies) {
			*latencies = append(*latencies, elapsed)
		}
		if write {
			stats.packetsIn++
			if emitted {
				stats.packetsOut++
				if _, err := writer.Write(out[:]); err != nil {
					return err
				}
			}
		}
	}
	withheld, err := call.Drain(tail)
	if err != nil {
		return err
	}
	if write {
		for i := range withheld {
			if _, err := writer.Write(tail[i][:]); err != nil {
				return err
			}
		}
		stats.packetsOut += withheld
	}
	return nil
}

// latencyCapacity sizes the latency samples before the loop: the known packet count of a file or synthetic input
// times the passes, or two hours of packets for standard input. Samples beyond it are not recorded.
func latencyCapacity(opts options, input io.Reader) int {
	packets := 2 * 60 * 60 * 50
	if opts.synthetic > 0 {
		packets = opts.synthetic
	} else if file, ok := input.(*os.File); ok && file != os.Stdin {
		if info, err := file.Stat(); err == nil {
			packets = int((info.Size() + noiseox.PacketSize - 1) / noiseox.PacketSize)
		}
	}
	return packets * opts.passes
}

func report(opts options, stats *loopStats, latencies []time.Duration, mallocs, bytes uint64) {
	fmt.Fprintf(os.Stderr, "input packets: %d\noutput packets: %d\n", stats.packetsIn, stats.packetsOut)
	if stats.padded > 0 {
		fmt.Fprintf(os.Stderr, "padded the final partial packet with %d silence bytes\n", stats.padded)
	}
	if len(latencies) > 0 {
		slices.Sort(latencies)
		var total time.Duration
		for _, latency := range latencies {
			total += latency
		}
		percentile := func(p float64) time.Duration {
			return latencies[min(len(latencies)-1, int(p*float64(len(latencies))))]
		}
		us := func(d time.Duration) float64 { return float64(d) / float64(time.Microsecond) }
		fmt.Fprintf(os.Stderr, "ProcessPacket latency over %d packets (%d passes), microseconds:\n", len(latencies), opts.passes)
		fmt.Fprintf(os.Stderr, "  min %.2f  p50 %.2f  p90 %.2f  p99 %.2f  p99.9 %.2f  max %.2f  mean %.2f\n",
			us(latencies[0]), us(percentile(0.50)), us(percentile(0.90)), us(percentile(0.99)),
			us(percentile(0.999)), us(latencies[len(latencies)-1]), us(total/time.Duration(len(latencies))))
		fmt.Fprintf(os.Stderr, "  slowest packet: %.3f %% of the 20 ms cadence\n",
			100*float64(latencies[len(latencies)-1])/float64(cadence))
	}
	fmt.Fprintf(os.Stderr, "heap allocations during the call loop: %d (%d bytes)\n", mallocs, bytes)
	if opts.pace {
		fmt.Fprintln(os.Stderr, "  (paced: the count includes the Go runtime's background allocations while the loop waits)")
	}
}

// runParallel runs calls independent calls, one Call and one goroutine each, over data looped to seconds of audio,
// and checks that they all produce the same output.
func runParallel(config noiseox.Config, data []byte, calls, seconds int) error {
	packets := len(data) / noiseox.PacketSize
	if packets == 0 {
		return errors.New("-parallel needs at least one whole packet")
	}
	perCall := seconds * int(time.Second/cadence)
	digests := make([]uint64, calls)
	errs := make([]error, calls)
	var ready, done sync.WaitGroup
	start := make(chan struct{})
	for index := range calls {
		ready.Add(1)
		done.Add(1)
		go func() {
			defer done.Done()
			call, err := noiseox.New(config)
			ready.Done()
			if err != nil {
				errs[index] = err
				return
			}
			defer call.Close()
			<-start
			digests[index], errs[index] = loop(call, data, packets, perCall)
		}()
	}
	ready.Wait()
	began := time.Now()
	close(start)
	done.Wait()
	wall := time.Since(began)
	if err := errors.Join(errs...); err != nil {
		return err
	}
	for index, digest := range digests {
		if digest != digests[0] {
			return fmt.Errorf("parallel call %d differs from call 0", index)
		}
	}
	audio := time.Duration(calls*perCall) * cadence
	fmt.Fprintf(os.Stderr, "parallel: %d calls of %d s each (GOMAXPROCS %d): wall %.3f s, real-time factor %.0f\n",
		calls, seconds, runtime.GOMAXPROCS(0), wall.Seconds(), audio.Seconds()/wall.Seconds())
	return nil
}

// loop streams count packets of data (repeating it) through call, drains it, and returns the FNV-1a digest of the
// output.
func loop(call *noiseox.Call, data []byte, packets, count int) (uint64, error) {
	var in, out [noiseox.PacketSize]byte
	var tail [noiseox.DelayPackets][noiseox.PacketSize]byte
	digest := uint64(0xCBF29CE484222325)
	add := func(packet []byte) {
		for _, b := range packet {
			digest = (digest ^ uint64(b)) * 0x100000001B3
		}
	}
	for index := range count {
		offset := (index % packets) * noiseox.PacketSize
		copy(in[:], data[offset:offset+noiseox.PacketSize])
		emitted, err := call.ProcessPacket(&in, &out)
		if err != nil {
			return 0, err
		}
		if emitted {
			add(out[:])
		}
	}
	withheld, err := call.Drain(&tail)
	if err != nil {
		return 0, err
	}
	for i := range withheld {
		add(tail[i][:])
	}
	return digest, nil
}

// synthetic returns packets of call-like mu-law audio: a quiet first second of noise, then alternating half-second
// bursts at two louder levels.
func synthetic(packets int) []byte {
	data := make([]byte, packets*noiseox.PacketSize)
	state := uint64(0x9E3779B97F4A7C15)
	for index := range packets {
		level := uint64(110)
		switch {
		case index < 50:
			level = 30
		case (index/25)%2 == 0:
			level = 60
		}
		for i := range noiseox.PacketSize {
			state ^= state >> 12
			state ^= state << 25
			state ^= state >> 27
			draw := (state * 0x2545F4914F6CDD1D) >> 32
			sign := byte(0)
			if draw>>31 == 1 {
				sign = 0x80
			}
			data[index*noiseox.PacketSize+i] = ^(sign | byte(draw%(level+1)))
		}
	}
	return data
}

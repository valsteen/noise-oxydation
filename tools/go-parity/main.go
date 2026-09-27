// Command go-parity drives the pinned Go behavior reference (github.com/sghaida/noise-cancelation at cfc7520) on
// headerless 8 kHz mu-law files so that its output and timing can be compared with noise-oxydation.
//
// It imports the reference module's public packages as a dependency and composes them in the order of the
// reference's end-to-end benchmark: mu-law decode, high-pass, STFT, noise estimation with a baseline that ends at
// the first frame reaching past the calibration duration, Log-MMSE, tonal transient gain, ISTFT, analyzer flush,
// synthesizer flush, trim to the input length, mu-law encode. No reference code is copied.
//
// Subcommands (run with `go run . <subcommand> -h` for flags):
//
//	enhance  enhance one mu-law file with SPP-MMSE or MCRA (tonal suppression enabled)
//	bench    per-packet latency on one goroutine and wall time of concurrent calls looping one input
//	u1       reference concern U1: the first output sample for an impulse at sample 0
//	u3       reference concern U3: MCRA with its baseline finished before any frame
package main

import (
	"errors"
	"flag"
	"fmt"
	"math"
	"os"
	"runtime"
	"sort"
	"sync"
	"time"

	"github.com/sghaida/noise-cancelation/audio"
	"github.com/sghaida/noise-cancelation/codec/mulaw"
	"github.com/sghaida/noise-cancelation/dsp/highpass"
	"github.com/sghaida/noise-cancelation/dsp/interference"
	"github.com/sghaida/noise-cancelation/dsp/noise"
	"github.com/sghaida/noise-cancelation/dsp/snr"
	"github.com/sghaida/noise-cancelation/dsp/stft"
	"github.com/sghaida/noise-cancelation/dsp/suppressor"
)

const (
	sampleRate    = 8000
	packetSamples = 160
	fftSize       = 256
	hopSize       = 128
	cutoffHz      = float32(80)
)

// noiseEstimator is the part of the reference estimators the composition uses.
type noiseEstimator interface {
	StartBaseline()
	FinishBaseline()
	Process(power []float32) []float32
}

func newEstimator(name string) (noiseEstimator, error) {
	switch name {
	case "spp-mmse":
		return noise.NewSPPMMSEEstimator(noise.DefaultSPPMMSEConfig()), nil
	case "mcra":
		return noise.NewMCRAEstimator(noise.DefaultMCRAConfig()), nil
	default:
		return nil, fmt.Errorf("unknown estimator %q (want spp-mmse or mcra)", name)
	}
}

// call is one reference pipeline instance.
type call struct {
	highPass    *highpass.Filter
	analyzer    *stft.Analyzer
	synthesizer *stft.Synthesizer
	estimator   noiseEstimator
	logMMSE     *suppressor.LogMMSE
	tones       *interference.TonalTransientDetector

	decoded         []float32
	encoded         []byte
	output          []byte
	keepOutput      bool
	checksum        uint64
	baselineSamples int
	frames          int
	baselineDone    bool
}

func newCall(estimatorName string, calibration time.Duration, keepOutput bool, capacity int) (*call, error) {
	analyzer, err := stft.New(fftSize, hopSize)
	if err != nil {
		return nil, err
	}
	synthesizer, err := stft.NewSynthesizer(fftSize, hopSize)
	if err != nil {
		return nil, err
	}
	estimator, err := newEstimator(estimatorName)
	if err != nil {
		return nil, err
	}
	estimator.StartBaseline()
	c := &call{
		highPass:        highpass.New(sampleRate, cutoffHz),
		analyzer:        analyzer,
		synthesizer:     synthesizer,
		estimator:       estimator,
		logMMSE:         suppressor.NewLogMMSE(suppressor.DefaultLogMMSEConfig(), snr.DefaultDecisionDirectedConfig()),
		tones:           interference.NewTonalTransientDetector(interference.DefaultTonalTransientConfig()),
		decoded:         make([]float32, packetSamples),
		encoded:         make([]byte, 0, fftSize),
		keepOutput:      keepOutput,
		baselineSamples: int(calibration.Seconds() * sampleRate),
	}
	if keepOutput {
		c.output = make([]byte, 0, capacity+fftSize)
	}
	return c, nil
}

func (c *call) processPacket(packet []byte) error {
	c.decoded = mulaw.DecodeMuLaw(c.decoded, packet)
	filtered, err := c.highPass.Process(c.decoded)
	if err != nil {
		return err
	}
	spectra, err := c.analyzer.Process(filtered)
	if err != nil {
		return err
	}
	for _, spectrum := range spectra {
		if err := c.processSpectrum(spectrum); err != nil {
			return err
		}
	}
	return nil
}

func (c *call) processSpectrum(spectrum audio.Spectrum) error {
	baseline := c.frames*hopSize+fftSize <= c.baselineSamples
	if !baseline && !c.baselineDone {
		c.estimator.FinishBaseline()
		c.baselineDone = true
	}
	power := spectrum.RecomputePower()
	noisePSD := c.estimator.Process(power)
	output := spectrum
	if !baseline {
		var err error
		if output, err = c.logMMSE.Process(spectrum, noisePSD); err != nil {
			return err
		}
		if err := interference.ApplyGain(&output, c.tones.Process(power).Gain); err != nil {
			return err
		}
	}
	samples, err := c.synthesizer.Process(output)
	if err != nil {
		return err
	}
	c.emit(samples)
	c.frames++
	return nil
}

func (c *call) emit(samples []float32) {
	c.encoded = mulaw.EncodeMuLaw(c.encoded[:0], samples)
	if c.keepOutput {
		c.output = append(c.output, c.encoded...)
		return
	}
	for _, value := range c.encoded {
		c.checksum = c.checksum*131 + uint64(value) + 1
	}
}

// finish flushes the analyzer and the synthesizer, as the reference compositions do at the end of a call.
func (c *call) finish() error {
	spectra, err := c.analyzer.Flush()
	if err != nil {
		return err
	}
	for _, spectrum := range spectra {
		if err := c.processSpectrum(spectrum); err != nil {
			return err
		}
	}
	if !c.baselineDone {
		c.estimator.FinishBaseline()
		c.baselineDone = true
	}
	c.emit(c.synthesizer.Flush())
	return nil
}

func readPackets(path string) ([][]byte, []byte, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, nil, err
	}
	if len(data) == 0 || len(data)%packetSamples != 0 {
		return nil, nil, fmt.Errorf("%s has %d bytes; expected whole 160-byte packets", path, len(data))
	}
	packets := make([][]byte, 0, len(data)/packetSamples)
	for start := 0; start < len(data); start += packetSamples {
		packets = append(packets, data[start:start+packetSamples])
	}
	return packets, data, nil
}

// enhanceAll runs one call over every packet and returns the output trimmed to the input length.
func enhanceAll(estimator string, calibration time.Duration, packets [][]byte) ([]byte, error) {
	c, err := newCall(estimator, calibration, true, len(packets)*packetSamples)
	if err != nil {
		return nil, err
	}
	for _, packet := range packets {
		if err := c.processPacket(packet); err != nil {
			return nil, err
		}
	}
	if err := c.finish(); err != nil {
		return nil, err
	}
	length := len(packets) * packetSamples
	if len(c.output) < length {
		return nil, fmt.Errorf("reference produced %d samples for %d input samples", len(c.output), length)
	}
	return c.output[:length], nil
}

func enhanceCommand(arguments []string) error {
	flags := flag.NewFlagSet("enhance", flag.ExitOnError)
	estimator := flags.String("estimator", "spp-mmse", "noise estimator: spp-mmse or mcra")
	input := flags.String("in", "", "headerless 8 kHz mu-law input")
	output := flags.String("out", "", "headerless mu-law output, trimmed to the input length")
	calibration := flags.Duration("calibration", 5*time.Second, "baseline (calibration) duration")
	_ = flags.Parse(arguments)
	if *input == "" || *output == "" {
		return errors.New("enhance needs -in and -out")
	}
	packets, _, err := readPackets(*input)
	if err != nil {
		return err
	}
	enhanced, err := enhanceAll(*estimator, *calibration, packets)
	if err != nil {
		return err
	}
	if err := os.WriteFile(*output, enhanced, 0o644); err != nil {
		return err
	}
	fmt.Printf("%s: %d packets enhanced with %s (tonal suppression on), %d bytes written to %s\n",
		*input, len(packets), *estimator, len(enhanced), *output)
	return nil
}

func microseconds(duration time.Duration) string {
	return fmt.Sprintf("%.2f", float64(duration.Nanoseconds())/1000)
}

func benchCommand(arguments []string) error {
	flags := flag.NewFlagSet("bench", flag.ExitOnError)
	input := flags.String("in", "", "headerless 8 kHz mu-law input")
	passes := flags.Int("passes", 20, "latency passes over the input")
	calls := flags.Int("calls", 100, "concurrent calls")
	seconds := flags.Int("seconds", 120, "audio seconds per concurrent call (the input is looped)")
	_ = flags.Parse(arguments)
	packets, _, err := readPackets(*input)
	if err != nil {
		return err
	}
	fmt.Printf("go %s %s/%s, GOMAXPROCS %d; input %s (%d packets)\n",
		runtime.Version(), runtime.GOOS, runtime.GOARCH, runtime.GOMAXPROCS(0), *input, len(packets))
	fmt.Printf("\nper-packet latency on one goroutine, %d passes (budget 20 ms)\n", *passes)
	fmt.Printf("  %-12s %8s %8s %8s %8s %8s %9s %9s\n", "estimator", "min", "p50", "p90", "p99", "p99.9", "max", "mean")
	for _, estimator := range []string{"spp-mmse", "mcra"} {
		durations := make([]time.Duration, 0, *passes*len(packets))
		for pass := -1; pass < *passes; pass++ {
			c, err := newCall(estimator, 5*time.Second, false, 0)
			if err != nil {
				return err
			}
			for _, packet := range packets {
				start := time.Now()
				err := c.processPacket(packet)
				elapsed := time.Since(start)
				if err != nil {
					return err
				}
				if pass >= 0 {
					durations = append(durations, elapsed)
				}
			}
			if err := c.finish(); err != nil {
				return err
			}
		}
		sort.Slice(durations, func(i, j int) bool { return durations[i] < durations[j] })
		percentile := func(perMille int) time.Duration { return durations[(len(durations)-1)*perMille/1000] }
		var total time.Duration
		for _, duration := range durations {
			total += duration
		}
		fmt.Printf("  %-12s %8s %8s %8s %8s %8s %9s %9s\n", estimator, microseconds(durations[0]),
			microseconds(percentile(500)), microseconds(percentile(900)), microseconds(percentile(990)),
			microseconds(percentile(999)), microseconds(durations[len(durations)-1]),
			microseconds(total/time.Duration(len(durations))))
	}

	callPackets := *seconds * sampleRate / packetSamples
	fmt.Printf("\n%d concurrent calls of %d s each (input looped), one goroutine per call\n", *calls, *seconds)
	for _, estimator := range []string{"spp-mmse", "mcra"} {
		runtime.GC()
		var ready, done sync.WaitGroup
		start := make(chan struct{})
		failures := make(chan error, *calls)
		ready.Add(*calls)
		done.Add(*calls)
		for range *calls {
			go func() {
				defer done.Done()
				c, err := newCall(estimator, 5*time.Second, false, 0)
				ready.Done()
				<-start
				if err != nil {
					failures <- err
					return
				}
				for index := range callPackets {
					if err := c.processPacket(packets[index%len(packets)]); err != nil {
						failures <- err
						return
					}
				}
				if err := c.finish(); err != nil {
					failures <- err
				}
			}()
		}
		ready.Wait()
		began := time.Now()
		close(start)
		done.Wait()
		wall := time.Since(began)
		close(failures)
		for err := range failures {
			return err
		}
		fmt.Printf("  %-12s wall %.3f s, %.3f ms per call-equivalent, real-time factor %.0fx\n", estimator,
			wall.Seconds(), wall.Seconds()*1000/float64(*calls), float64(*calls**seconds)/wall.Seconds())
	}
	return nil
}

func u1Command(arguments []string) error {
	flags := flag.NewFlagSet("u1", flag.ExitOnError)
	inputPath := flags.String("write-input", "", "optionally write the impulse input here (headerless mu-law)")
	_ = flags.Parse(arguments)
	input := make([]byte, 50*packetSamples)
	for index := range input {
		input[index] = 0xFF // mu-law zero
	}
	input[0] = 0x80 // +32124, the largest positive level
	if *inputPath != "" {
		if err := os.WriteFile(*inputPath, input, 0o644); err != nil {
			return err
		}
	}
	packets := make([][]byte, 0, 50)
	for start := 0; start < len(input); start += packetSamples {
		packets = append(packets, input[start:start+packetSamples])
	}
	output, err := enhanceAll("spp-mmse", 5*time.Second, packets)
	if err != nil {
		return err
	}
	fmt.Printf("U1: impulse 0x80 (%d) at input sample 0, then silence; reference output samples 0..3 decode to", mulaw.ToPCM16(0x80))
	for _, value := range output[:4] {
		fmt.Printf(" %d", mulaw.ToPCM16(value))
	}
	fmt.Println()
	return nil
}

func u3Command([]string) error {
	power := make([]float32, fftSize/2+1)
	for bin := range power {
		power[bin] = 1e-3 * float32(1+bin%7)
	}
	for _, name := range []string{"mcra", "spp-mmse"} {
		estimator, err := newEstimator(name)
		if err != nil {
			return err
		}
		estimator.StartBaseline()
		estimator.FinishBaseline()
		fmt.Printf("U3: %-8s baseline finished before any frame; largest noise estimate after frames", name)
		for frame := 1; frame <= 500; frame++ {
			estimate := estimator.Process(power)
			if frame == 1 || frame == 10 || frame == 500 {
				largest := 0.0
				for _, value := range estimate {
					largest = math.Max(largest, float64(value))
				}
				fmt.Printf(" %d: %.4g", frame, largest)
			}
		}
		fmt.Println(" (input power 1e-3 to 7e-3 per bin)")
	}
	return nil
}

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: go run . enhance|bench|u1|u3 [flags]")
		os.Exit(2)
	}
	commands := map[string]func([]string) error{"enhance": enhanceCommand, "bench": benchCommand, "u1": u1Command, "u3": u3Command}
	command, ok := commands[os.Args[1]]
	if !ok {
		fmt.Fprintf(os.Stderr, "unknown subcommand %q\n", os.Args[1])
		os.Exit(2)
	}
	if err := command(os.Args[2:]); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}

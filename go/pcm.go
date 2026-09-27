package noiseoxydation

import (
	"fmt"
	"math"
)

// PCMProcessor adapts a 160-sample normalized float32 call loop to the
// packet-oriented Rust pipeline. Process and Reset match the reference Go
// dsp.Processor interface. Finish drains the delayed audio at call end.
// The returned slices are owned by the caller. This compatibility path
// converts through G.711 μ-law and allocates an output slice; use Call
// directly at the μ-law packet boundary for the leaner path.
type PCMProcessor struct {
	call *Call
}

func NewPCMProcessor(config Config) (*PCMProcessor, error) {
	call, err := New(config)
	if err != nil {
		return nil, err
	}
	return &PCMProcessor{call: call}, nil
}

func (p *PCMProcessor) Process(samples []float32) ([]float32, error) {
	if len(samples) != PacketBytes {
		return nil, fmt.Errorf("noise oxydation: PCM Process requires %d samples, got %d", PacketBytes, len(samples))
	}
	var input [PacketBytes]byte
	for i, sample := range samples {
		if math.IsNaN(float64(sample)) || math.IsInf(float64(sample), 0) {
			return nil, fmt.Errorf("noise oxydation: non-finite PCM sample at index %d", i)
		}
		input[i] = encodePCM(sample)
	}
	var output [OutputBytes]byte
	batch, err := p.call.Process(&input, &output)
	if err != nil {
		return nil, err
	}
	return decodeBatch(&output, batch), nil
}

// Finish must be called once when the stream ends; the reference Processor
// interface has no end-of-call method, so the owner must add this call.
func (p *PCMProcessor) Finish() ([]float32, error) {
	var output [OutputBytes]byte
	batch, err := p.call.Finish(&output)
	if err != nil {
		return nil, err
	}
	return decodeBatch(&output, batch), nil
}

// Reset satisfies the reference Processor interface. Close is terminal;
// resetting an already closed processor has no effect.
func (p *PCMProcessor) Reset() { _ = p.call.Reset() }

func (p *PCMProcessor) Close() error { return p.call.Close() }

func decodeBatch(output *[OutputBytes]byte, batch Batch) []float32 {
	valid := batch.Count * PacketBytes
	if batch.Count > 0 {
		valid -= PacketBytes - batch.FinalValid
	}
	decoded := make([]float32, valid)
	for i := range decoded {
		decoded[i] = decodePCM(output[i])
	}
	return decoded
}

// These conversion rules mirror this repository's Rust G.711 codec. They
// keep the adapter independent of the reference Go module.
func decodePCM(code byte) float32 {
	value := ^code
	magnitude := (int(value&0x0f)<<3 + 132) << ((value & 0x70) >> 4)
	if value&0x80 == 0 {
		return float32(magnitude-132) / 32768
	}
	return float32(132-magnitude) / 32768
}

func encodePCM(sample float32) byte {
	pcm := int(math.Round(float64(sample) * 32768))
	if pcm > 32767 {
		pcm = 32767
	} else if pcm < -32768 {
		pcm = -32768
	}
	mask := byte(0xff)
	if pcm < 0 {
		pcm = -pcm
		mask = 0x7f
	}
	biased := min(pcm, 32635) + 132
	segment := 0
	for segment < 7 && biased > 0xff<<segment {
		segment++
	}
	return byte(segment<<4|(biased>>(segment+3))&0x0f) ^ mask
}

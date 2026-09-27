package noiseoxydation

import (
	"math"
	"slices"
	"testing"
	"time"
)

// The pinned Go reference exports this method shape as dsp.Processor.
var _ interface {
	Process([]float32) ([]float32, error)
	Reset()
} = (*PCMProcessor)(nil)

func TestPCMCodecPreservesDecodedMuLaw(t *testing.T) {
	for code := 0; code < 256; code++ {
		want := decodePCM(byte(code))
		if got := decodePCM(encodePCM(want)); got != want {
			t.Fatalf("code %d: got %v, want %v", code, got, want)
		}
	}
}

func TestPCMAdapterMatchesPacketCallInBothModes(t *testing.T) {
	for _, mode := range []ProcessingMode{Conservative, ExperimentalLowDelay} {
		config := Config{LearningDuration: time.Second, Mode: mode}
		adapter, err := NewPCMProcessor(config)
		if err != nil {
			t.Fatal(err)
		}
		direct, err := New(config)
		if err != nil {
			t.Fatal(err)
		}
		var firstOutput []float32
		var firstOutputSnapshot []float32
		for run := 0; run < 2; run++ {
			var got, want []float32
			for packetIndex := 0; packetIndex < 110; packetIndex++ {
				var packet [PacketBytes]byte
				var pcm [PacketBytes]float32
				for i := range packet {
					packet[i] = byte(packetIndex + i)
					pcm[i] = decodePCM(packet[i])
				}
				out, err := adapter.Process(pcm[:])
				if err != nil {
					t.Fatal(err)
				}
				var raw [OutputBytes]byte
				batch, err := direct.Process(&packet, &raw)
				if err != nil {
					t.Fatal(err)
				}
				if !slices.Equal(out, decodeBatch(&raw, batch)) {
					t.Fatalf("mode %d packet %d differs from direct call", mode, packetIndex)
				}
				if packetIndex == 1 {
					wantPackets := map[ProcessingMode]int{Conservative: 0, ExperimentalLowDelay: 1}[mode]
					if len(out) != wantPackets*PacketBytes {
						t.Fatalf("mode %d packet 2 emitted %d samples", mode, len(out))
					}
				}
				if packetIndex == 2 {
					firstOutput = out
					firstOutputSnapshot = slices.Clone(out)
				}
				got = append(got, out...)
				want = append(want, decodeBatch(&raw, batch)...)
			}
			var raw [OutputBytes]byte
			tail, err := adapter.Finish()
			if err != nil {
				t.Fatal(err)
			}
			batch, err := direct.Finish(&raw)
			if err != nil {
				t.Fatal(err)
			}
			got = append(got, tail...)
			want = append(want, decodeBatch(&raw, batch)...)
			if len(got) != 110*PacketBytes || !slices.Equal(got, want) {
				t.Fatalf("mode %d run %d output mismatch", mode, run)
			}
			if !slices.Equal(firstOutput, firstOutputSnapshot) {
				t.Fatalf("mode %d returned output changed after later calls", mode)
			}
			if _, err := adapter.Finish(); err == nil {
				t.Fatal("second finish succeeded")
			}
			adapter.Reset()
			if err := direct.Reset(); err != nil {
				t.Fatal(err)
			}
		}
		if err := adapter.Close(); err != nil {
			t.Fatal(err)
		}
		if err := direct.Close(); err != nil {
			t.Fatal(err)
		}
	}
}

func TestPCMAdapterRejectsInvalidInputWithoutAdvancing(t *testing.T) {
	adapter, err := NewPCMProcessor(DefaultConfig())
	if err != nil {
		t.Fatal(err)
	}
	defer adapter.Close()
	if _, err := adapter.Process(make([]float32, PacketBytes-1)); err == nil {
		t.Fatal("short packet accepted")
	}
	pcm := make([]float32, PacketBytes)
	pcm[12] = float32(math.NaN())
	if _, err := adapter.Process(pcm); err == nil {
		t.Fatal("NaN accepted")
	}
	pcm[12] = 0
	if out, err := adapter.Process(pcm); err != nil || len(out) != 0 {
		t.Fatalf("first valid packet: %d samples, %v", len(out), err)
	}
	if err := adapter.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := adapter.Process(pcm); err == nil {
		t.Fatal("closed processor accepted input")
	}
}

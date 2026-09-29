package noiseoxydation

import (
	"bytes"
	"errors"
	"testing"
)

func TestPacketAndDrainCapacityRetriesPreserveState(t *testing.T) {
	processor := New()
	defer processor.Close()
	packet := make([]byte, PacketBytes)
	for index := range packet {
		packet[index] = 0xff
	}
	output := make([]byte, MaxPacketOutputBytes)
	if written, err := processor.Push(packet, output); err != nil || written != 0 {
		t.Fatalf("first push = %d, %v; want 0, nil", written, err)
	}
	short := make([]byte, 127)
	if written, err := processor.Push(packet, short); written != 0 || !errors.Is(err, ErrOutputTooSmall) {
		t.Fatalf("short push = %d, %v; want no output and ErrOutputTooSmall", written, err)
	} else {
		var capacity *CapacityError
		if !errors.As(err, &capacity) || capacity.Required != 128 {
			t.Fatalf("short push capacity = %#v; want 128", capacity)
		}
	}
	retriedPacketOutput := make([]byte, 128)
	if written, err := processor.Push(packet, retriedPacketOutput); err != nil || written != 128 {
		t.Fatalf("retry push = %d, %v; want 128, nil", written, err)
	}
	shortTail := make([]byte, 191)
	if written, err := processor.Drain(shortTail); written != 0 || !errors.Is(err, ErrOutputTooSmall) {
		t.Fatalf("short drain = %d, %v; want no output and ErrOutputTooSmall", written, err)
	} else {
		var capacity *CapacityError
		if !errors.As(err, &capacity) || capacity.Required != 192 {
			t.Fatalf("short drain capacity = %#v; want 192", capacity)
		}
	}
	retriedTail := make([]byte, 192)
	if written, err := processor.Drain(retriedTail); err != nil || written != 192 {
		t.Fatalf("drain retry = %d, %v; want 192, nil", written, err)
	}
	if written, err := processor.Drain(make([]byte, MaxDrainOutputBytes)); err != nil || written != 0 {
		t.Fatalf("repeat drain = %d, %v; want 0, nil", written, err)
	}
	if _, err := processor.Push(packet, output); !errors.Is(err, ErrStreamDrained) {
		t.Fatalf("push after drain = %v; want ErrStreamDrained", err)
	}

	fresh := New()
	defer fresh.Close()
	if written, err := fresh.Push(packet, output); err != nil || written != 0 {
		t.Fatalf("fresh first push = %d, %v; want 0, nil", written, err)
	}
	freshPacketOutput := make([]byte, MaxPacketOutputBytes)
	if written, err := fresh.Push(packet, freshPacketOutput); err != nil || written != 128 {
		t.Fatalf("fresh second push = %d, %v; want 128, nil", written, err)
	}
	freshTail := make([]byte, MaxDrainOutputBytes)
	if written, err := fresh.Drain(freshTail); err != nil || written != 192 {
		t.Fatalf("fresh drain = %d, %v; want 192, nil", written, err)
	}
	if !bytes.Equal(retriedPacketOutput, freshPacketOutput[:128]) || !bytes.Equal(retriedTail, freshTail[:192]) {
		t.Fatal("capacity retries differ from processing the same packets without retries")
	}
}

func TestPacketLengthResetAndEmptyStream(t *testing.T) {
	processor := New()
	defer processor.Close()
	for _, packetLength := range []int{PacketBytes - 1, PacketBytes + 1} {
		if _, err := processor.Push(make([]byte, packetLength), make([]byte, MaxPacketOutputBytes)); !errors.Is(err, ErrInvalidPacketLength) {
			t.Fatalf("packet length %d error = %v; want ErrInvalidPacketLength", packetLength, err)
		}
	}
	packet := make([]byte, PacketBytes)
	output := make([]byte, MaxPacketOutputBytes)
	if written, err := processor.Push(packet, output); err != nil || written != 0 {
		t.Fatalf("push after malformed packet = %d, %v; want 0, nil", written, err)
	}
	if err := processor.Reset(); err != nil {
		t.Fatalf("reset = %v", err)
	}
	if written, err := processor.Push(packet, output); err != nil || written != 0 {
		t.Fatalf("push after reset = %d, %v; want 0, nil", written, err)
	}
	if written, err := processor.Drain(make([]byte, MaxDrainOutputBytes)); err != nil || written != PacketBytes {
		t.Fatalf("drain after reset = %d, %v; want %d, nil", written, err, PacketBytes)
	}
	if err := processor.Reset(); err != nil {
		t.Fatalf("reset after drain = %v", err)
	}
	if written, err := processor.Drain(make([]byte, MaxDrainOutputBytes)); err != nil || written != 0 {
		t.Fatalf("empty drain = %d, %v; want 0, nil", written, err)
	}
	if _, err := processor.Push(packet, output); !errors.Is(err, ErrStreamDrained) {
		t.Fatalf("push after empty drain = %v; want ErrStreamDrained", err)
	}
	empty := New()
	defer empty.Close()
	if written, err := empty.Drain(make([]byte, MaxDrainOutputBytes)); err != nil || written != 0 {
		t.Fatalf("new empty drain = %d, %v; want 0, nil", written, err)
	}
	if _, err := empty.Push(packet, output); !errors.Is(err, ErrStreamDrained) {
		t.Fatalf("push after New -> empty Drain = %v; want ErrStreamDrained", err)
	}
}

func TestCloseAndIndependentProcessors(t *testing.T) {
	first := New()
	second := New()
	packet := make([]byte, PacketBytes)
	for index := range packet {
		packet[index] = 0xff
	}
	output := make([]byte, MaxPacketOutputBytes)
	if written, err := first.Push(packet, output); err != nil || written != 0 {
		t.Fatalf("first push = %d, %v", written, err)
	}
	if written, err := second.Push(packet, output); err != nil || written != 0 {
		t.Fatalf("second push = %d, %v", written, err)
	}
	if err := first.Close(); err != nil {
		t.Fatalf("first close = %v", err)
	}
	if err := first.Close(); err != nil {
		t.Fatalf("repeated close = %v", err)
	}
	if _, err := first.Push(packet, output); !errors.Is(err, ErrClosed) {
		t.Fatalf("push after close = %v; want ErrClosed", err)
	}
	if _, err := first.Drain(make([]byte, MaxDrainOutputBytes)); !errors.Is(err, ErrClosed) {
		t.Fatalf("drain after close = %v; want ErrClosed", err)
	}
	if err := first.Reset(); !errors.Is(err, ErrClosed) {
		t.Fatalf("reset after close = %v; want ErrClosed", err)
	}
	if written, err := second.Drain(make([]byte, MaxDrainOutputBytes)); err != nil || written != PacketBytes {
		t.Fatalf("second drain = %d, %v; want %d, nil", written, err, PacketBytes)
	}
	if err := second.Close(); err != nil {
		t.Fatalf("second close = %v", err)
	}
}

func TestCopiedProcessorSharesHandleLifetime(t *testing.T) {
	processor := New()
	copy := *processor
	if err := processor.Close(); err != nil {
		t.Fatalf("close original = %v", err)
	}
	if err := copy.Close(); err != nil {
		t.Fatalf("close copy = %v", err)
	}
	if _, err := copy.Push(make([]byte, PacketBytes), make([]byte, MaxPacketOutputBytes)); !errors.Is(err, ErrClosed) {
		t.Fatalf("push through copied processor = %v; want ErrClosed", err)
	}
}

func TestPacketPhasesStayBoundedAndResetMatchesFreshProcessor(t *testing.T) {
	processor := New()
	defer processor.Close()
	packets := make([][]byte, 8)
	for packetIndex := range packets {
		packets[packetIndex] = make([]byte, PacketBytes)
		for byteIndex := range packets[packetIndex] {
			packets[packetIndex][byteIndex] = byte(packetIndex*41 + byteIndex*17)
		}
	}
	var stream []byte
	output := make([]byte, MaxPacketOutputBytes)
	maximumPush := 0
	for _, packet := range packets {
		written, err := processor.Push(packet, output)
		if err != nil {
			t.Fatalf("push: %v", err)
		}
		if written > MaxPacketOutputBytes {
			t.Fatalf("push wrote %d bytes; bound is %d", written, MaxPacketOutputBytes)
		}
		maximumPush = max(maximumPush, written)
		stream = append(stream, output[:written]...)
	}
	if maximumPush != MaxPacketOutputBytes {
		t.Fatalf("largest push = %d; want exercised bound %d", maximumPush, MaxPacketOutputBytes)
	}
	tail := make([]byte, MaxDrainOutputBytes)
	written, err := processor.Drain(tail)
	if err != nil {
		t.Fatalf("drain: %v", err)
	}
	if written > MaxDrainOutputBytes {
		t.Fatalf("drain wrote %d bytes; bound is %d", written, MaxDrainOutputBytes)
	}
	stream = append(stream, tail[:written]...)
	if got, want := len(stream), len(packets)*PacketBytes; got != want {
		t.Fatalf("stream length = %d; want %d", got, want)
	}

	if err := processor.Reset(); err != nil {
		t.Fatalf("reset after drain: %v", err)
	}
	if _, err := processor.Push(packets[0], output); err != nil {
		t.Fatalf("push before active reset: %v", err)
	}
	if _, err := processor.Push(packets[1], output); err != nil {
		t.Fatalf("second push before active reset: %v", err)
	}
	if err := processor.Reset(); err != nil {
		t.Fatalf("reset active stream: %v", err)
	}
	resetPackets := packets[2:5]
	got := collectPackets(t, processor, resetPackets)
	fresh := New()
	defer fresh.Close()
	want := collectPackets(t, fresh, resetPackets)
	if len(got) != len(want) {
		t.Fatalf("reset stream length = %d; fresh stream length = %d", len(got), len(want))
	}
	for index := range got {
		if got[index] != want[index] {
			t.Fatalf("reset stream differs from fresh processor at output byte %d", index)
		}
	}
}

func collectPackets(t *testing.T, processor *Processor, packets [][]byte) []byte {
	t.Helper()
	output := make([]byte, MaxPacketOutputBytes)
	var stream []byte
	for _, packet := range packets {
		written, err := processor.Push(packet, output)
		if err != nil {
			t.Fatalf("push: %v", err)
		}
		stream = append(stream, output[:written]...)
	}
	tail := make([]byte, MaxDrainOutputBytes)
	written, err := processor.Drain(tail)
	if err != nil {
		t.Fatalf("drain: %v", err)
	}
	return append(stream, tail[:written]...)
}

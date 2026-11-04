package main

import (
	"encoding/binary"
	"fmt"
	"net/netip"
)

// Định dạng NetFlow phiên bản 5: tiêu đề 24 byte + count bản ghi x 48 byte, thứ tự byte mạng (big-endian).
//
//	Tiêu đề: version(2) count(2) sys_uptime(4) unix_secs(4) unix_nsecs(4) flow_sequence(4)
//	         engine_type(1) engine_id(1) sampling_interval(2)
//	Bản ghi: srcaddr(4) dstaddr(4) nexthop(4) input(2) output(2) dPkts(4) dOctets(4) first(4) last(4)
//	         srcport(2) dstport(2) pad1(1) tcp_flags(1) prot(1) tos(1) src_as(2) dst_as(2)
//	         src_mask(1) dst_mask(1) pad2(2)
const (
	headerSize = 24
	recordSize = 48
	maxRecords = 30 // giới hạn của phiên bản 5
)

var protoName = map[uint8]string{6: "TCP", 17: "UDP", 1: "ICMP"}

// Flow – các trường của một bản ghi mà bài này dùng tới.
type Flow struct {
	Src, Dst     netip.Addr
	SPort, DPort uint16
	Proto        uint8
	Pkts, Bytes  uint32
	First, Last  uint32
}

// Header – các trường tiêu đề bài này dùng tới.
type Header struct {
	Count    uint16
	Uptime   uint32
	Secs     uint32
	Nsecs    uint32
	Seq      uint32
	EngineID uint8
}

// packPacket – đóng gói tiêu đề và các bản ghi thành một datagram.
func packPacket(h Header, flows []Flow) []byte {
	b := make([]byte, headerSize+recordSize*len(flows))
	be := binary.BigEndian
	be.PutUint16(b[0:], 5)
	be.PutUint16(b[2:], uint16(len(flows)))
	be.PutUint32(b[4:], h.Uptime)
	be.PutUint32(b[8:], h.Secs)
	be.PutUint32(b[12:], h.Nsecs)
	be.PutUint32(b[16:], h.Seq)
	b[20], b[21] = 0, h.EngineID // engine_type, engine_id; sampling_interval = 0
	for i, f := range flows {
		r := b[headerSize+i*recordSize:]
		s4, d4 := f.Src.As4(), f.Dst.As4()
		copy(r[0:4], s4[:])
		copy(r[4:8], d4[:]) // nexthop r[8:12] = 0.0.0.0
		be.PutUint16(r[12:], 1)
		be.PutUint16(r[14:], 2)
		be.PutUint32(r[16:], f.Pkts)
		be.PutUint32(r[20:], f.Bytes)
		be.PutUint32(r[24:], f.First)
		be.PutUint32(r[28:], f.Last)
		be.PutUint16(r[32:], f.SPort)
		be.PutUint16(r[34:], f.DPort)
		if f.Proto == 6 {
			r[37] = 0x1B // tcp_flags
		}
		r[38] = f.Proto
		r[44], r[45] = 24, 27 // src_mask, dst_mask
	}
	return b
}

// parsePacket – giải mã một datagram; trả lỗi nếu gói không hợp lệ.
func parsePacket(b []byte) (Header, []Flow, error) {
	be := binary.BigEndian
	if len(b) < headerSize {
		return Header{}, nil, fmt.Errorf("quá ngắn (%d byte)", len(b))
	}
	if v := be.Uint16(b[0:]); v != 5 {
		return Header{}, nil, fmt.Errorf("phiên bản %d, không phải 5", v)
	}
	h := Header{Count: be.Uint16(b[2:]), Uptime: be.Uint32(b[4:]), Secs: be.Uint32(b[8:]),
		Nsecs: be.Uint32(b[12:]), Seq: be.Uint32(b[16:]), EngineID: b[21]}
	if h.Count < 1 || h.Count > maxRecords || len(b) != headerSize+int(h.Count)*recordSize {
		return Header{}, nil, fmt.Errorf("độ dài %d không khớp count=%d", len(b), h.Count)
	}
	flows := make([]Flow, h.Count)
	for i := range flows {
		r := b[headerSize+i*recordSize:]
		flows[i] = Flow{
			Src: netip.AddrFrom4([4]byte(r[0:4])), Dst: netip.AddrFrom4([4]byte(r[4:8])),
			Pkts: be.Uint32(r[16:]), Bytes: be.Uint32(r[20:]), First: be.Uint32(r[24:]), Last: be.Uint32(r[28:]),
			SPort: be.Uint16(r[32:]), DPort: be.Uint16(r[34:]), Proto: r[38],
		}
	}
	return h, flows, nil
}

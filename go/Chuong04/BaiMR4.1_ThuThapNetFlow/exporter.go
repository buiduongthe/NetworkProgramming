package main

import (
	"fmt"
	"net"
	"net/netip"
	"os"
	"time"
)

var (
	hosts    = []string{"10.20.0.11", "10.20.0.12", "10.20.0.13", "10.20.0.14", "10.20.1.5", "10.20.1.6"}
	services = []struct {
		Dst   string
		Port  uint16
		Proto uint8
	}{{"10.20.1.194", 443, 6}, {"10.20.1.194", 80, 6}, {"10.20.1.195", 5432, 6},
		{"10.20.1.196", 53, 17}, {"203.0.113.10", 443, 6}}
)

// lcg – x = (x*1103515245 + 12345) mod 2^31, lấy 15 bit cao; giống hệt bản Python để sinh
// cùng dữ liệu. Đủ để sinh dữ liệu thử, KHÔNG dùng cho bảo mật.
type lcg struct{ x uint64 }

func (g *lcg) next() uint64 {
	g.x = (g.x*1103515245 + 12345) & 0x7FFFFFFF
	return g.x >> 16
}

func makeFlows(n int) []Flow {
	g := &lcg{x: 2026}
	flows := make([]Flow, 0, n)
	for i := 0; i < n; i++ {
		src := netip.MustParseAddr(hosts[g.next()%uint64(len(hosts))])
		svc := services[g.next()%uint64(len(services))]
		pkts := 1 + g.next()%200
		octets := pkts * (60 + g.next()%1400)
		sport := 32768 + g.next()%28000
		first := g.next() % 60000
		last := first + g.next()%5000
		flows = append(flows, Flow{Src: src, Dst: netip.MustParseAddr(svc.Dst), SPort: uint16(sport),
			DPort: svc.Port, Proto: svc.Proto, Pkts: uint32(pkts), Bytes: uint32(octets),
			First: uint32(first), Last: uint32(last)})
	}
	return flows
}

// runExporter – gửi các luồng thành từng gói tối đa 30 bản ghi; dropK > 0 thì cố ý bỏ gói thứ dropK.
func runExporter(addr string, n, dropK int) {
	conn, err := net.Dial("udp", addr) // UDP "kết nối": chỉ ghi nhớ địa chỉ đích
	if err != nil {
		fmt.Println("[xuat] Lỗi:", err)
		os.Exit(1)
	}
	defer conn.Close()
	flows := makeFlows(n)
	seq := uint32(0)
	for k, start := 1, 0; start < len(flows); k, start = k+1, start+maxRecords {
		end := min(start+maxRecords, len(flows))
		chunk := flows[start:end]
		now := time.Now()
		data := packPacket(Header{Uptime: uint32(120000 + k*1000), Secs: uint32(now.Unix()),
			Nsecs: uint32(now.Nanosecond()), Seq: seq, EngineID: 1}, chunk)
		if k == dropK {
			fmt.Printf("[xuat] Gói #%d: %d bản ghi, seq=%d -> BỎ (giả lập mất gói)\n", k, len(chunk), seq)
		} else {
			conn.Write(data)
			fmt.Printf("[xuat] Gói #%d: %d bản ghi, seq=%d, %d byte\n", k, len(chunk), seq, len(data))
		}
		seq += uint32(len(chunk)) // flow_sequence đếm tổng số luồng đã xuất
		time.Sleep(50 * time.Millisecond)
	}
	conn.Write([]byte("xin chao, day khong phai NetFlow"))
	fmt.Println("[xuat] Gửi thêm 1 datagram rác để thử bộ thu")
}

package main

import (
	"errors"
	"fmt"
	"net"
	"os"
	"os/signal"
	"sort"
	"time"
)

type svcKey struct {
	Port  uint16
	Proto uint8
}

type engineKey struct {
	Addr     string
	EngineID uint8
}

// runCollector – nhận datagram NetFlow v5, kiểm tra, phát hiện mất gói, tổng hợp và in báo cáo.
func runCollector(addr string, idle time.Duration) {
	pc, err := net.ListenPacket("udp", addr)
	if err != nil {
		fmt.Println("[thu-thap] Lỗi:", err)
		os.Exit(1)
	}
	fmt.Printf("[thu-thap] Nghe NetFlow v5 tại UDP %s\n", addr)
	sig := make(chan os.Signal, 1)
	signal.Notify(sig, os.Interrupt)
	go func() { <-sig; pc.SetReadDeadline(time.Now()) }() // Ctrl+C: dừng vòng đọc

	bySrc := map[string]uint64{}
	bySvc := map[svcKey]*[2]uint64{} // (cổng, giao thức) -> [số luồng, byte]
	expected := map[engineKey]uint32{}
	var ok, bad, flows, lost int
	var total uint64
	buf := make([]byte, 2048) // gói v5 lớn nhất 24 + 30*48 = 1464 byte
	started := false
	for {
		n, from, err := pc.ReadFrom(buf)
		if err != nil {
			var ne net.Error
			if errors.As(err, &ne) && ne.Timeout() && started {
				fmt.Printf("[thu-thap] Nhàn rỗi %.1f s, kết thúc\n", idle.Seconds())
			}
			break
		}
		started = true
		pc.SetReadDeadline(time.Now().Add(idle)) // sau gói đầu tiên mới bắt đầu đếm nhàn rỗi
		h, recs, err := parsePacket(buf[:n])
		if err != nil {
			bad++
			fmt.Printf("[thu-thap] Loại gói từ %s: %v\n", from, err)
			continue
		}
		key := engineKey{from.(*net.UDPAddr).IP.String(), h.EngineID}
		if want, seen := expected[key]; seen && h.Seq != want {
			gap := int(h.Seq) - int(want)
			lost += max(gap, 0)
			fmt.Printf("[thu-thap] MẤT GÓI: chờ seq=%d, nhận seq=%d, thiếu %d luồng\n", want, h.Seq, gap)
		}
		expected[key] = h.Seq + uint32(h.Count)
		ok++
		flows += int(h.Count)
		fmt.Printf("[thu-thap] Gói seq=%d: %d luồng\n", h.Seq, h.Count)
		for _, r := range recs {
			bySrc[r.Src.String()] += uint64(r.Bytes)
			k := svcKey{r.DPort, r.Proto}
			if bySvc[k] == nil {
				bySvc[k] = &[2]uint64{}
			}
			bySvc[k][0]++
			bySvc[k][1] += uint64(r.Bytes)
			total += uint64(r.Bytes)
		}
	}
	pc.Close()
	fmt.Printf("[thu-thap] Hợp lệ %d gói, loại %d gói, mất %d luồng\n", ok, bad, lost)
	fmt.Printf("[thu-thap] Đã thu %d luồng, tổng %d byte\n", flows, total)
	if total == 0 {
		return
	}
	fmt.Println("[thu-thap] Nguồn dùng nhiều byte nhất:")
	srcs := make([]string, 0, len(bySrc))
	for s := range bySrc {
		srcs = append(srcs, s)
	}
	sort.Slice(srcs, func(i, j int) bool {
		if bySrc[srcs[i]] != bySrc[srcs[j]] {
			return bySrc[srcs[i]] > bySrc[srcs[j]]
		}
		return srcs[i] < srcs[j]
	})
	for _, s := range srcs[:min(3, len(srcs))] {
		fmt.Printf("  %-12s %9d byte (%.1f%%)\n", s, bySrc[s], float64(bySrc[s])*100/float64(total))
	}
	fmt.Println("[thu-thap] Dịch vụ theo số luồng:")
	keys := make([]svcKey, 0, len(bySvc))
	for k := range bySvc {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool {
		a, b := keys[i], keys[j]
		if bySvc[a][0] != bySvc[b][0] {
			return bySvc[a][0] > bySvc[b][0]
		}
		if a.Port != b.Port {
			return a.Port < b.Port
		}
		return a.Proto < b.Proto
	})
	for _, k := range keys {
		name := fmt.Sprintf("%d/%s", k.Port, protoName[k.Proto])
		fmt.Printf("  %-9s %3d luồng %9d byte\n", name, bySvc[k][0], bySvc[k][1])
	}
}

package main

// Các kịch bản UDP, unicast, broadcast, multicast, anycast trên bộ mô phỏng.

import (
	"fmt"
	"slices"
	"strings"
)

const MS = 1000 // 1 ms = 1000 µs

func showPath(n *Net, a, b string) {
	fmt.Printf("  Đường %s→%s: %s (chi phí %d)\n", a, b, strings.Join(n.Path(a, b), " → "), n.dist[a][b])
}

// ---------------- UDP ----------------

// udp – H2 gửi 12 datagram tới H6, cách nhau 10 ms. Liên kết R2–R3 mất gói ngẫu nhiên và có trễ
// dao động (jitter) nên datagram có thể mất hoặc đến sai thứ tự; UDP không gửi lại.
func udp(n *Net, lossSet bool) {
	l := n.links[[2]string{"R2", "R3"}]
	if !lossSet {
		l.Loss = 200 // mặc định 20 %
	}
	l.Jitter = 15 * MS // trễ thêm ngẫu nhiên 0..15 ms
	n.hopTrace = false
	fmt.Printf("Liên kết R2–R3: mất gói %d.%d %%, trễ dao động 0–15 ms\n", l.Loss/10, l.Loss%10)
	showPath(n, "H2", "H6")
	var got []int
	expect, late := 1, 0
	n.onDeliver = func(node string, p *Packet) {
		k := p.N
		got = append(got, k)
		var note string
		switch {
		case k == expect:
			note = "đúng thứ tự"
		case k > expect:
			var miss []string
			for i := expect; i < k; i++ {
				miss = append(miss, fmt.Sprintf("#%d", i))
			}
			note = "đến sớm, đang thiếu " + strings.Join(miss, ", ")
		default:
			note = "ĐẾN MUỘN – sai thứ tự"
			late++
		}
		expect = max(expect, k+1)
		n.Log(fmt.Sprintf("H6 nhận datagram #%-2d %s", k, note), node)
	}
	for i := 1; i <= 12; i++ {
		n.At(int64(i-1)*10*MS, func() {
			n.Log(fmt.Sprintf("H2 gửi datagram #%d (200 byte)", i), "H2")
			p := NewPacket("uni", "UDP", "H2", "H6", fmt.Sprintf("UDP #%d", i), 200)
			p.N = i
			n.Send("H2", p)
		})
	}
	n.Run()
	var lost []string
	for i := 1; i <= 12; i++ {
		if !slices.Contains(got, i) {
			lost = append(lost, fmt.Sprintf("#%d", i))
		}
	}
	ls := strings.Join(lost, ", ")
	if ls == "" {
		ls = "không"
	}
	fmt.Println()
	fmt.Printf("Tóm tắt UDP: gửi 12, nhận %d, mất %d (%s), sai thứ tự %d\n", len(got), len(lost), ls, late)
	fmt.Println("  UDP không xác nhận, không gửi lại, không sắp xếp lại:")
	fmt.Println("  ứng dụng tự xử lý (đánh số thứ tự, bỏ qua hoặc yêu cầu lại).")
}

// ---------------- Unicast ----------------
func msgPacket(cast, kind, src, dst, label string, size int, msg string) *Packet {
	p := NewPacket(cast, kind, src, dst, label, size)
	p.Msg = msg
	return p
}

func unicast(n *Net) {
	fmt.Println("Bảng định tuyến của R1 (đích → chặng kế, chi phí):")
	for _, h := range []string{"H2", "H5"} {
		fmt.Printf("  %-3s → %-3s chi phí %d\n", h, n.nh["R1"][h], n.dist["R1"][h])
	}
	showPath(n, "H1", "H2")
	showPath(n, "H1", "H5")
	n.onDeliver = func(node string, p *Packet) {
		n.Log(fmt.Sprintf("%s nhận \"%s\" từ %s (TTL còn %d)", node, p.Msg, p.Src, p.TTL), node)
		if p.Reply {
			n.Send(node, msgPacket("uni", "UNI", node, p.Src, "UNI "+node+"→"+p.Src, 100, "chào "+p.Src))
		}
	}
	n.At(0, func() { n.Send("H1", msgPacket("uni", "UNI", "H1", "H2", "UNI H1→H2", 100, "xin chào H2")) })
	n.At(5*MS, func() {
		p := msgPacket("uni", "UNI", "H1", "H5", "UNI H1→H5", 100, "xin chào H5")
		p.Reply = true
		n.Send("H1", p)
	})
	n.At(50*MS, func() {
		n.Log("Liên kết R1–R2 hỏng → các router tính lại đường", "R1")
		n.SetLink("R1", "R2", false)
		showPath(n, "H1", "H5")
		n.Send("H1", msgPacket("uni", "UNI", "H1", "H5", "UNI H1→H5", 100, "lần 2"))
	})
	n.Run()
	fmt.Println()
	fmt.Println("Tóm tắt unicast: mỗi gói chỉ tới đúng một đích; switch chỉ")
	fmt.Println("  chuyển ra đúng một cổng; router giảm TTL, chọn chặng kế theo")
	fmt.Println("  chi phí nhỏ nhất; đường đổi khi liên kết hỏng.")
}

// ---------------- Broadcast ----------------
func broadcast(n *Net) {
	n.onDeliver = func(node string, p *Packet) {
		n.Log(fmt.Sprintf("%s nhận quảng bá \"%s\" từ %s", node, p.Msg, p.Src), node)
	}
	n.At(0, func() {
		n.Send("H1", msgPacket("bc", "BC", "H1", "192.168.1.255", "BC 192.168.1.255", 60, "Ai có 192.168.1.13?"))
	})
	n.At(10*MS, func() {
		n.Send("H4", msgPacket("bc", "BC", "H4", "255.255.255.255", "BC 255.255.255.255", 60, "DHCP DISCOVER"))
	})
	n.Run()
	fmt.Println()
	fmt.Println("Tóm tắt broadcast: switch gửi tràn mọi cổng trong LAN; router")
	fmt.Println("  chặn nên H4, H5, H6 không nhận quảng bá của H1 và LAN A không")
	fmt.Println("  nhận quảng bá của H4 (miền quảng bá = một LAN).")
}

// ---------------- Multicast ----------------
const G = "239.1.1.1"

func multicast(n *Net) {
	n.onDeliver = func(node string, p *Packet) {
		n.Log(fmt.Sprintf("%s nhận %s của nhóm %s", node, p.Msg, G), node)
	}
	join := func(h string, yes bool) func() {
		return func() {
			what, msg := "rời", "Leave"
			if yes {
				n.member[h] = append(n.member[h], G)
				what, msg = "tham gia", "Report"
			} else {
				i := slices.Index(n.member[h], G)
				n.member[h] = slices.Delete(n.member[h], i, i+1)
			}
			n.Log(fmt.Sprintf("%s %s nhóm %s: gửi IGMP %s", h, what, G, msg), h)
			p := NewPacket("igmp", "IGMP", h, G, "IGMP "+msg+" "+G, 60)
			p.Join = yes
			n.Send(h, p)
		}
	}
	send := func(k int) func() {
		return func() {
			n.Log(fmt.Sprintf("H1 gửi luồng #%d tới nhóm %s", k, G), "H1")
			n.Send("H1", msgPacket("mc", "MC", "H1", G, fmt.Sprintf("MC %s #%d", G, k), 500, fmt.Sprintf("luồng #%d", k)))
		}
	}
	n.At(0, join("H2", true))
	n.At(5*MS, send(1))
	n.At(20*MS, join("H5", true))
	n.At(40*MS, send(2))
	n.At(70*MS, join("H6", true))
	n.At(75*MS, join("H2", false))
	n.At(90*MS, send(3))
	n.Run()
	fmt.Println()
	fmt.Println("Tóm tắt multicast: chỉ thành viên nhóm nhận dữ liệu; switch")
	fmt.Println("  (IGMP snooping) chỉ chuyển ra cổng có thành viên; router chỉ")
	fmt.Println("  chuyển sang LAN có thành viên; mỗi liên kết chỉ mang một bản")
	fmt.Println("  sao dù phía sau có nhiều thành viên.")
}

// ---------------- Anycast ----------------
const A = "10.53.0.53"

func anycast(n *Net) {
	n.anycast[A] = []string{"H3", "H6"}
	fmt.Printf("Địa chỉ anycast %s (DNS) do H3 và H6 cùng quảng bá.\n", A)
	for _, c := range []string{"H1", "H5"} {
		var costs []string
		for _, s := range n.anycast[A] {
			costs = append(costs, fmt.Sprintf("%s chi phí %d", s, n.dist[c][s]))
		}
		fmt.Printf("  %s: %s → chọn %s\n", c, strings.Join(costs, ", "), n.target(c, NewPacket("any", "ANY", c, A, "", 0)))
	}
	n.onDeliver = func(node string, p *Packet) {
		if p.Cast == "any" {
			n.Log(fmt.Sprintf("%s (phục vụ %s) nhận \"%s\" từ %s", node, A, p.Msg, p.Src), node)
			n.Send(node, msgPacket("uni", "UNI", node, p.Src, "UNI "+node+"→"+p.Src, 120, "trả lời từ "+node))
		} else {
			n.Log(fmt.Sprintf("%s nhận \"%s\"", node, p.Msg), node)
		}
	}
	query := func(c string, k int) func() {
		return func() {
			n.Log(fmt.Sprintf("%s gửi truy vấn #%d tới %s", c, k, A), c)
			n.Send(c, msgPacket("any", "ANY", c, A, "ANY "+c+"→"+A, 80, fmt.Sprintf("truy vấn #%d", k)))
		}
	}
	n.At(0, query("H1", 1))
	n.At(2*MS, query("H5", 1))
	n.At(40*MS, func() {
		n.Log("H3 ngừng hoạt động → rút quảng bá "+A, "H3")
		n.SetNode("H3", false)
		best := n.target("H1", NewPacket("any", "ANY", "H1", A, "", 0))
		fmt.Printf("  H1: còn H6 chi phí %d → chọn %s\n", n.dist["H1"]["H6"], best)
	})
	n.At(41*MS, query("H1", 2))
	n.Run()
	fmt.Println()
	fmt.Println("Tóm tắt anycast: cùng một địa chỉ, gói tới máy phục vụ gần nhất")
	fmt.Println("  theo chi phí định tuyến; khi H3 hỏng, truy vấn của H1 được")
	fmt.Println("  chuyển sang H6 (dự phòng tự động).")
}

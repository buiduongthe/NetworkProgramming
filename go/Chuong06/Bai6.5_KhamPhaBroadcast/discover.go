// Bài 6.5 – Khám phá server chat trong LAN bằng broadcast: phía client (DISCOVER).
//
// Cài đặt Mã giả 6.7, hàm DISCOVER (Mục 6.4.2, Giáo trình Lập trình mạng). Kế thừa Mã giả 4.4
// (UDP_REQUEST): gửi một yêu cầu nhưng thu thập NHIỀU phản hồi đến hết thời gian chờ.
//
// Chạy:  out/app discover [wait_ms] [disc_port] [địa_chỉ_broadcast]
//
//	(mặc định 2000 9999 255.255.255.255)
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"math/rand/v2"
	"net"
	"strconv"
	"syscall"
	"time"
)

type discoverReq struct {
	Type    string `json:"type"`
	Service string `json:"service"`
	ID      int    `json:"id"`
}

type discoverResp struct {
	ID   int    `json:"id"`
	Port int    `json:"port"`
	Name string `json:"name"`
}

type found struct {
	ip   string
	port int
	name string
}

// discover – DISCOVER(wait_ms).
func discover(waitMs int, discPort int, bcast string) []found {
	// SOCKET(INET, DGRAM) + SETOPT(s, BROADCAST, TRUE): bắt buộc để được gửi tới địa chỉ broadcast
	lc := net.ListenConfig{Control: setOpt(syscall.SO_BROADCAST)}
	pc, err := lc.ListenPacket(context.Background(), "udp4", "0.0.0.0:0")
	if err != nil {
		fail("[client] Không mở được socket UDP: " + err.Error())
	}
	defer pc.Close()                                                               // CLOSE(s)
	q := discoverReq{Type: "discover", Service: "chat", ID: rand.IntN(999999) + 1} // NEXT_ID()
	data, _ := json.Marshal(q)
	dst, err := net.ResolveUDPAddr("udp4", net.JoinHostPort(bcast, strconv.Itoa(discPort)))
	if err != nil {
		fail("Địa chỉ broadcast không hợp lệ: " + bcast)
	}
	if _, err := pc.WriteTo(data, dst); err != nil { // SENDTO(s, …, ("255.255.255.255", DISC_PORT))
		fail("[client] Gửi lỗi: " + err.Error())
	}
	fmt.Printf("[client] Phát discover id=%d tới %s:%d\n", q.ID, bcast, discPort)
	fmt.Printf("[client] Thu thập phản hồi trong %d ms\n", waitMs)
	var list []found
	deadline := time.Now().Add(time.Duration(waitMs) * time.Millisecond)
	// RECV_TIMEOUT = deadline − NOW(): Go đặt thẳng hạn chót tuyệt đối, không lo giá trị 0 = chờ mãi
	pc.SetReadDeadline(deadline)
	buf := make([]byte, 2048)
	for time.Now().Before(deadline) {
		n, addr, err := pc.ReadFrom(buf) // RECVFROM(s, 2048)
		if err != nil {
			break // CATCH Timeout: BREAK
		}
		var r discoverResp
		if json.Unmarshal(buf[:n], &r) != nil {
			continue // CATCH JsonError: sai định dạng, bỏ
		}
		if r.ID == q.ID {
			list = append(list, found{addr.(*net.UDPAddr).IP.String(), r.Port, r.Name})
		}
	}
	return list
}

func discoverMain(args []string) {
	waitMs := intArg(args, 0, 2000)
	discPort := portArg(args, 1, DISC_PORT)
	bcast := "255.255.255.255"
	if len(args) > 2 {
		bcast = args[2]
	}
	list := discover(waitMs, discPort, bcast)
	fmt.Printf("[client] Tìm thấy %d server:\n", len(list))
	for _, f := range list {
		fmt.Printf("[client]   %s tại %s:%d\n", f.name, f.ip, f.port)
	}
}

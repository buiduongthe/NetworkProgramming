// Bài 6.6 – Phát thông báo dịch vụ qua multicast (ANNOUNCER).
//
// Cài đặt Mã giả 6.8, thủ tục ANNOUNCER (Mục 6.4.3, Giáo trình Lập trình mạng). Kế thừa Mã giả 6.7:
// thay địa chỉ broadcast bằng địa chỉ nhóm 239.255.42.99 (dải quản trị cục bộ 239.0.0.0/8).
// Tham số count (bổ sung để minh họa): dừng sau count lần phát; 0 = phát mãi.
//
// Chạy:  out/app announcer [service_port] [tên] [period_ms] [count] [mport]
//
//	(mặc định 9200 server-chat 5000 0 9998)
package main

import (
	"encoding/json"
	"fmt"
	"math/rand/v2"
	"net"
	"os"
	"os/signal"
	"syscall"
	"time"
)

type announce struct {
	Type  string `json:"type"`
	Name  string `json:"name"`
	Port  int    `json:"port"`
	TtlMs int    `json:"ttl_ms"`
}

// announcer – ANNOUNCER(service_port, name, period_ms).
func announcer(servicePort int, name string, periodMs int, count int, mport int) {
	conn, err := net.ListenUDP("udp4", nil) // SOCKET(INET, DGRAM)
	if err != nil {
		fail("[announcer] Không mở được socket UDP: " + err.Error())
	}
	defer conn.Close()
	rc, _ := conn.SyscallConn()
	setIPOpt(rc, syscall.IP_MULTICAST_TTL, 1)  // chỉ trong mạng con
	setIPOpt(rc, syscall.IP_MULTICAST_LOOP, 1) // nhận cả trên máy gửi
	group := &net.UDPAddr{IP: net.ParseIP(GROUP), Port: mport}
	fmt.Printf("[announcer] Phát tới nhóm %s:%d, chu kỳ %d ms\n", GROUP, mport, periodMs)
	n := 0
	for count == 0 || n < count {
		a := announce{Type: "announce", Name: name, Port: servicePort, TtlMs: 3 * periodMs}
		data, _ := json.Marshal(a)
		conn.WriteTo(data, group) // SENDTO(s, …, (GROUP, MPORT))
		n++
		fmt.Printf("[announcer] Lần %d: announce %s, cổng %d, ttl_ms=%d\n", n, name, servicePort, a.TtlMs)
		jitter := rand.IntN(periodMs/10 + 1)                          // RANDOM(0, period_ms / 10)
		time.Sleep(time.Duration(periodMs+jitter) * time.Millisecond) // có nhiễu
	}
	fmt.Printf("[announcer] Dừng phát sau %d lần\n", n)
}

func announcerMain(args []string) {
	ch := make(chan os.Signal, 1) // Ctrl+C: in "Dừng" rồi kết thúc
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		fmt.Println("[announcer] Dừng")
		os.Exit(0)
	}()
	name := "server-chat"
	if len(args) > 1 {
		name = args[1]
	}
	announcer(portArg(args, 0, 9200), name, intArg(args, 2, 5000), intArg(args, 3, 0), portArg(args, 4, MPORT))
}

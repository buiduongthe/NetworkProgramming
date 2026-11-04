// Bài 6.5 – Khám phá server chat trong LAN bằng broadcast: phía server (RESPONDER).
//
// Cài đặt Mã giả 6.7, thủ tục RESPONDER (Mục 6.4.2, Giáo trình Lập trình mạng): nhận yêu cầu discover,
// chờ ngẫu nhiên 0–200 ms (giãn phản hồi) rồi trả lời bằng unicast kèm cổng dịch vụ.
// Trong server thật, RESPONDER chạy như một goroutine nền bên cạnh vòng chấp nhận kết nối.
//
// Chạy:  out/app responder [service_port] [tên] [disc_port]   (mặc định 9200 server-chat 9999)
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

type discoverQuery struct {
	Type    string `json:"type"`
	Service string `json:"service"`
	ID      any    `json:"id"` // giữ nguyên kiểu của id để trả lại đúng giá trị
}

type reply struct {
	ID   any    `json:"id"`
	Port int    `json:"port"`
	Name string `json:"name"`
}

// responder – RESPONDER(service_port, name).
func responder(servicePort int, name string, discPort int) {
	// SOCKET(INET, DGRAM); bật REUSEADDR (cho nhiều responder trên một máy) rồi BIND(s, "0.0.0.0", DISC_PORT)
	lc := net.ListenConfig{Control: setOpt(syscall.SO_REUSEADDR)}
	pc, err := lc.ListenPacket(context.Background(), "udp4", "0.0.0.0:"+strconv.Itoa(discPort))
	if err != nil {
		fail("[responder] Không mở được cổng: " + err.Error())
	}
	defer pc.Close()
	onInterrupt("[responder] Dừng")
	fmt.Printf("[responder] %s, cổng dịch vụ %d, chờ discover tại cổng %d\n", name, servicePort, discPort)
	buf := make([]byte, 2048)
	for {
		n, addr, err := pc.ReadFrom(buf) // AWAIT RECVFROM_ASYNC(s, 2048)
		if err != nil {
			continue
		}
		var q discoverQuery
		if json.Unmarshal(buf[:n], &q) != nil {
			continue // JsonError: sai định dạng, bỏ
		}
		if q.Type == "discover" && q.Service == "chat" {
			delay := rand.IntN(201) // RANDOM(0, 200)
			fmt.Printf("[responder] discover id=%v từ %s, trả lời sau %d ms\n", q.ID, addr, delay)
			time.Sleep(time.Duration(delay) * time.Millisecond) // AWAIT DELAY_ASYNC: giãn phản hồi
			r, _ := json.Marshal(reply{ID: q.ID, Port: servicePort, Name: name})
			pc.WriteTo(r, addr) // SENDTO(s, …, addr): trả lời unicast
		}
	}
}

func responderMain(args []string) {
	servicePort := portArg(args, 0, 9200)
	name := "server-chat"
	if len(args) > 1 {
		name = args[1]
	}
	responder(servicePort, name, portArg(args, 2, DISC_PORT))
}

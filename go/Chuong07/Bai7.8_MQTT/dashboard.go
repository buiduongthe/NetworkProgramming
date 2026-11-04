// Bài 7.8 – Bảng theo dõi đăng ký bộ lọc có ký tự đại diện (Mã giả 7.9, PROCEDURE DASHBOARD; Mục 7.5.5).
//
// Đăng ký nhamay/+/nhietdo (QoS 0, ON_TEMP) và nhamay/+/trangthai (QoS 1, ON_STATUS), rồi chạy vòng sự kiện;
// gửi PINGREQ khi im lặng quá nửa thời gian keep alive.
//
// Chạy:  out/app dashboard [host] [port] [keep_alive_s] [client_id]   (mặc định localhost 1883 60 dashboard-01)
//
//	Hai client cùng client_id thì broker ngắt client cũ, nên bảng thứ hai cần client_id khác.
package main

import (
	"fmt"
	"os"
	"os/signal"
	"sync/atomic"
)

func onTemp(topic, payload string, qos int, retain bool) {
	fmt.Printf("[dashboard] Nhiệt độ  %s: %s\n", topic, payload)
}

func onStatus(topic, payload string, qos int, retain bool) {
	note := ""
	if retain {
		note = ", tin giữ lại"
	}
	fmt.Printf("[dashboard] Trạng thái %s: %s (QoS %d%s)\n", topic, payload, qos, note)
}

func dashboardMain(args []string) {
	host := "localhost"
	if len(args) >= 1 {
		host = args[0]
	}
	port := portArg(args, 1, 1883)
	keepAlive := intArg(args, 2, 60)
	clientID := "dashboard-01"
	if len(args) >= 4 {
		clientID = args[3]
	}
	c := &MqttClient{}
	if err := c.connect(host, port, clientID, keepAlive, nil); err != nil {
		fail("[dashboard] Không kết nối được broker: " + err.Error())
	}
	fmt.Printf("[dashboard] Đã kết nối broker %s:%d là '%s' (MQTT 5.0, keep alive %d s)\n", host, port, clientID, keepAlive)
	subs := []struct {
		flt string
		qos int
		fn  func(string, string, int, bool)
	}{{"nhamay/+/nhietdo", 0, onTemp}, {"nhamay/+/trangthai", 1, onStatus}}
	for _, s := range subs {
		granted, err := c.subscribe(s.flt, s.qos, s.fn)
		if err != nil {
			fail("[dashboard] " + err.Error())
		}
		fmt.Printf("[dashboard] SUBSCRIBE %s (QoS %d) -> SUBACK, QoS được cấp %d\n", s.flt, s.qos, granted)
	}
	var stopping atomic.Bool
	ch := make(chan os.Signal, 1) // Ctrl+C: gửi DISCONNECT rồi kết thúc
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		stopping.Store(true)
		c.disconnect()
		fmt.Println("[dashboard] Đã gửi DISCONNECT")
		os.Exit(0)
	}()
	err := c.runEventLoop(func() { fmt.Println("[dashboard] PINGREQ -> PINGRESP") }) // RUN_EVENT_LOOP(c)
	if stopping.Load() {
		select {} // đang ngắt kết nối theo Ctrl+C: chờ goroutine kia kết thúc chương trình
	}
	fmt.Printf("[dashboard] %v\n", err)
}

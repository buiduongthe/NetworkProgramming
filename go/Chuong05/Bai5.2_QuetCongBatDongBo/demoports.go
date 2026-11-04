package main

import (
	"fmt"
	"net"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"time"
)

// Mở các cổng minh họa trên 127.0.0.1 để chạy thử chương trình quét (không thuộc mã giả).
//   - Cổng "mở": socket lắng nghe bình thường, chấp nhận và đóng ngay mọi kết nối.
//   - Cổng "bị lọc" (mô phỏng): socket lắng nghe với backlog 1 không bao giờ ACCEPT; chương trình
//     tự lấp đầy hàng đợi kết nối, sau đó Linux bỏ qua gói SYN mới nên phía quét hết thời gian chờ,
//     giống cổng bị tường lửa lọc. (Trên Windows hàng đợi đầy thường trả RST, tức cổng đóng.)
// Các cổng còn lại trong dải không có chương trình lắng nghe nên ở trạng thái đóng.

func openPort(port int) (net.Listener, error) {
	ln, err := net.Listen("tcp4", "127.0.0.1:"+strconv.Itoa(port))
	if err != nil {
		return nil, err
	}
	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				return
			}
			conn.Close()
		}
	}()
	return ln, nil
}

func filteredPort(port int) ([]net.Conn, net.Listener, error) {
	ln, err := net.Listen("tcp4", "127.0.0.1:"+strconv.Itoa(port))
	if err != nil {
		return nil, nil, err
	}
	if err := setBacklog(ln, 1); err != nil { // hàng đợi rất ngắn, không bao giờ ACCEPT
		return nil, ln, err
	}
	var fillers []net.Conn
	for range 8 { // lấp đầy hàng đợi
		c, err := net.DialTimeout("tcp4", "127.0.0.1:"+strconv.Itoa(port), 300*time.Millisecond)
		if err != nil {
			break // SYN bị bỏ qua: hàng đợi đã đầy
		}
		fillers = append(fillers, c)
	}
	return fillers, ln, nil
}

func parseList(text string) []int {
	var out []int
	for _, x := range strings.Split(text, ",") {
		if n, err := strconv.Atoi(x); err == nil {
			out = append(out, n)
		}
	}
	return out
}

func joinInts(xs []int) string {
	parts := make([]string, len(xs))
	for i, x := range xs {
		parts[i] = strconv.Itoa(x)
	}
	return strings.Join(parts, ", ")
}

// RunDemoPorts – mở cổng minh họa và chờ Ctrl+C.
func RunDemoPorts(args []string) int {
	opened, filtered := []int{9003, 9011, 9017}, []int{9005, 9008, 9013, 9019}
	if len(args) > 0 {
		opened = parseList(args[0])
	}
	if len(args) > 1 {
		filtered = parseList(args[1])
	}
	for _, p := range opened {
		if _, err := openPort(p); err != nil {
			fmt.Printf("[demo] Không mở được cổng %d: %v\n", p, err)
			return 1
		}
	}
	for _, p := range filtered {
		if _, _, err := filteredPort(p); err != nil {
			fmt.Printf("[demo] Không tạo được cổng bị lọc %d: %v\n", p, err)
			return 1
		}
	}
	fmt.Printf("[demo] Cổng mở: %s\n", joinInts(opened))
	fmt.Printf("[demo] Cổng bị lọc (mô phỏng): %s\n", joinInts(filtered))
	fmt.Println("[demo] Nhấn Ctrl+C để dừng")
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	<-ch
	fmt.Println("[demo] Dừng") // các socket được hệ điều hành đóng khi tiến trình kết thúc
	return 0
}

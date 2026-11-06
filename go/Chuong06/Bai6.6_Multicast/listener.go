// Bài 6.6 – Nhận thông báo dịch vụ qua multicast (LISTENER).
//
// Cài đặt Mã giả 6.8, thủ tục LISTENER (Mục 6.4.3, Giáo trình Lập trình mạng): bật REUSEADDR trước
// BIND để nhiều chương trình trên cùng máy cùng nghe một cổng, tham gia nhóm (IGMP join), cập nhật
// bộ nhớ đệm cache và xóa dịch vụ hết hạn. Tham số run_ms (bổ sung để minh họa): nghe trong run_ms
// rồi in bộ nhớ đệm và kết thúc; 0 = nghe mãi.
//
// Chạy:  out/app listener [run_ms] [mport]      (mặc định 0 9998)
package main

import (
	"encoding/json"
	"fmt"
	"net"
	"os"
	"os/signal"
	"strings"
	"sync"
	"syscall"
	"time"
)

// entry: một mục của bộ nhớ đệm – (addr.ip, a.port, hạn).
type entry struct {
	ip      string
	port    int
	expires time.Time
}

// Cache: tên dịch vụ → entry; order giữ thứ tự thêm vào để in ra ổn định.
type Cache struct {
	mu    sync.Mutex
	items map[string]entry
	order []string
}

func (c *Cache) put(name string, e entry) bool {
	_, existed := c.items[name]
	if !existed {
		c.order = append(c.order, name)
	}
	c.items[name] = e
	return !existed
}

// removeExpired – REMOVE_EXPIRED(cache, now): xóa dịch vụ đã quá hạn ttl_ms.
func (c *Cache) removeExpired(now time.Time) {
	kept := c.order[:0]
	for _, name := range c.order {
		if c.items[name].expires.Before(now) {
			delete(c.items, name)
			fmt.Printf("[listener] Hết hạn, xóa: %s\n", name)
		} else {
			kept = append(kept, name)
		}
	}
	c.order = kept
}

func (c *Cache) String() string {
	parts := []string{}
	for _, name := range c.order {
		e := c.items[name]
		parts = append(parts, fmt.Sprintf("%s tại %s:%d", name, e.ip, e.port))
	}
	if len(parts) == 0 {
		return "(trống)"
	}
	return strings.Join(parts, ", ")
}

// listener – LISTENER(cache).
func listener(cache *Cache, runMs int, mport int) {
	// SOCKET, SETOPT(REUSEADDR), BIND, SETOPT(ADD_MEMBERSHIP, (GROUP, ANY_INTERFACE)):
	// net.ListenMulticastUDP làm cả bốn bước; giao diện nil = để hệ điều hành chọn (ANY_INTERFACE).
	group := &net.UDPAddr{IP: net.ParseIP(GROUP), Port: mport}
	conn, err := net.ListenMulticastUDP("udp4", nil, group)
	if err != nil {
		fail("[listener] Không tham gia được nhóm: " + err.Error())
	}
	defer conn.Close()
	// Go đặt IP_MULTICAST_LOOP = 0 cho socket này. Trên Windows tùy chọn ấy áp dụng ở phía NHẬN nên listener không
	// thấy gói của announcer chạy cùng máy; bật lại (trên Linux nó chỉ ảnh hưởng phía gửi nên bật lại vô hại).
	if rc, e := conn.SyscallConn(); e == nil {
		rc.Control(func(fd uintptr) { _ = setsockoptInt(fd, syscall.IPPROTO_IP, syscall.IP_MULTICAST_LOOP, 1) })
	}
	fmt.Printf("[listener] Tham gia nhóm %s, nghe cổng %d\n", GROUP, mport)
	end := time.Now().Add(time.Duration(runMs) * time.Millisecond)
	if runMs > 0 {
		conn.SetReadDeadline(end) // nghe đến thời điểm end
	}
	buf := make([]byte, 2048)
	for {
		n, addr, err := conn.ReadFromUDP(buf) // RECVFROM(s, 2048)
		if err != nil {
			return // hết thời gian nghe
		}
		var a announce
		if json.Unmarshal(buf[:n], &a) != nil {
			continue // JsonError: sai định dạng, bỏ
		}
		cache.mu.Lock()
		if a.Type == "announce" {
			if cache.put(a.Name, entry{addr.IP.String(), a.Port, time.Now().Add(time.Duration(a.TtlMs) * time.Millisecond)}) {
				fmt.Printf("[listener] Thêm %s tại %s:%d\n", a.Name, addr.IP, a.Port)
			}
		}
		cache.removeExpired(time.Now()) // dịch vụ hết hạn: xóa
		cache.mu.Unlock()
	}
}

func listenerMain(args []string) {
	cache := &Cache{items: map[string]entry{}}
	ch := make(chan os.Signal, 1) // Ctrl+C: in bộ nhớ đệm rồi kết thúc
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		cache.mu.Lock()
		fmt.Printf("[listener] Bộ nhớ đệm: %s\n", cache)
		os.Exit(0)
	}()
	listener(cache, intArg(args, 0, 0), portArg(args, 1, MPORT))
	cache.mu.Lock()
	fmt.Printf("[listener] Bộ nhớ đệm: %s\n", cache)
}

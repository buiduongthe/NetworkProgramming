package main

import (
	"bufio"
	"fmt"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"time"
)

// Giao diện dòng lệnh dùng lớp ChatClient (Mã giả 5.5).
// Tầng "giao diện" chỉ đăng ký hai sự kiện OnMessage, OnStatus và gọi SendMessage, Disconnect.
// Mỗi dòng nhập được gửi thành một tin vào phòng "chung"; hai lệnh riêng:
//
//	/quit        ngắt kết nối và thoát
//	/cho <giây>  tạm dừng đọc bàn phím (dùng trong kịch bản chạy minh họa)
//
// Bàn phím được đọc trên một goroutine phụ và chuyển vào kênh lines, nên vòng nhận vẫn chạy
// trong lúc chờ người dùng gõ.

func showMessage(me string, msg map[string]any) { // trình xử lý sự kiện OnMessage
	str := func(k string) string { v, _ := GetString(msg, k); return v }
	switch str("type") {
	case "msg":
		fmt.Printf("[%s] %s: %s\n", me, str("from"), str("text"))
	case "sys":
		fmt.Printf("[%s] (hệ thống) %s\n", me, str("text"))
	case "welcome":
		var names []string
		if list, ok := msg["users"].([]any); ok {
			for _, u := range list {
				names = append(names, fmt.Sprint(u))
			}
		}
		fmt.Printf("[%s] Server chào mừng; đang trực tuyến: %s\n", me, strings.Join(names, ", "))
	case "error":
		fmt.Printf("[%s] Lỗi %d: %s\n", me, GetInt(msg, "code"), str("text"))
	default:
		fmt.Printf("[%s] Nhận: %s\n", me, JsonBytes(msg))
	}
}

// RunChatCli – cli <tên> [host] [port].
func RunChatCli(args []string) int {
	if len(args) < 1 {
		fmt.Println("Cách dùng: go run . cli <tên> [host] [port]")
		return 1
	}
	me := args[0]
	host, port, err := ParseEndpoint(args[1:], "127.0.0.1", 9200)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	c := NewChatClient()
	c.OnMessage = func(msg map[string]any) { showMessage(me, msg) } // đăng ký sự kiện
	c.OnStatus = func(text string) { fmt.Printf("[%s] Trạng thái: %s\n", me, text) }
	if err := c.Connect(host, port, me); err != nil {
		fmt.Printf("[%s] Không kết nối được: %v\n", me, err)
		return 1
	}
	defer func() {
		c.Disconnect()
		fmt.Printf("[%s] Đã ngắt kết nối\n", me)
	}()
	lines := make(chan string)
	go func() { // goroutine đọc bàn phím
		sc := bufio.NewScanner(os.Stdin)
		for sc.Scan() {
			lines <- strings.TrimRight(sc.Text(), "\r")
		}
		close(lines) // hết dữ liệu vào (Ctrl+D)
	}()
	stop := make(chan os.Signal, 1)
	signal.Notify(stop, os.Interrupt) // Ctrl+C: ngắt kết nối như /quit
	for {
		select {
		case <-stop:
			return 0
		case line, ok := <-lines:
			if !ok || line == "/quit" {
				return 0
			}
			if strings.HasPrefix(line, "/cho ") {
				sec, _ := strconv.ParseFloat(strings.TrimSpace(line[5:]), 64)
				time.Sleep(time.Duration(sec * float64(time.Second)))
			} else if line != "" {
				if err := c.SendMessage("chung", line); err != nil { // AWAIT SEND_MESSAGE(c, room, text)
					fmt.Printf("[%s] Lỗi gửi: %v\n", me, err)
				}
			}
		}
	}
}

package main

import (
	"bufio"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net"
	"os"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"
)

// Pattern – 14 ký tự, 18 byte UTF-8.
const Pattern = "Mạng máy tính "

// makeText tạo chuỗi đúng n ký tự, có chữ tiếng Việt (2–3 byte mỗi ký tự trong UTF-8).
func makeText(n int) string {
	runes := []rune(strings.Repeat(Pattern, n/utf8.RuneCountInString(Pattern)+1))
	return string(runes[:n])
}

func sha256Hex(text string) string {
	sum := sha256.Sum256([]byte(text))
	return hex.EncodeToString(sum[:])
}

// RunFrameClient – Echo Client dùng khung JSON: gửi dòng nhập từ bàn phím và một thông điệp lớn.
// Cài đặt Mã giả 3.2 (Mục 3.2.3) với vòng gửi nhận thô được thay bằng SEND_JSON/RECV_JSON
// của Mã giả 3.6 (Mục 3.4.3). Tham số thứ ba (tùy chọn) là số ký tự của thông điệp lớn gửi
// sau khi hết dữ liệu nhập, ví dụ 200000: nhờ RECV_EXACT, phản hồi được nhận đủ dù đến thành nhiều phần.
func RunFrameClient(args []string) int {
	host, port, err := ParseEndpoint(args[:min(len(args), 2)], "127.0.0.1", 9000)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	big := 0
	if len(args) > 2 {
		if big, err = strconv.Atoi(args[2]); err != nil || big < 0 {
			fmt.Printf("Số ký tự không hợp lệ: %s\n", args[2])
			return 2
		}
	}
	conn, err := SafeConnect(host, port, 3000)
	if err != nil {
		fmt.Printf("[client] Không kết nối được %s:%d: %v\n", host, port, err)
		return 1
	}
	defer func() {
		conn.Close() // CLOSE(s)
		fmt.Println("[client] Đã đóng kết nối")
	}()
	fmt.Printf("[client] Đã kết nối %s:%d. Nhập dòng cần gửi (Ctrl+D để kết thúc).\n", host, port)
	// exchange gửi {"text": text} và nhận phản hồi, có thời gian chờ nhận 5000 ms (RECV_TIMEOUT)
	exchange := func(text string) (string, error) {
		if err := SendJSON(conn, map[string]any{"text": text}); err != nil { // SEND_JSON thay cho SEND_ALL thô
			return "", err
		}
		conn.SetReadDeadline(time.Now().Add(5000 * time.Millisecond))
		resp, err := RecvJSON(conn) // RECV_JSON: luôn nhận đủ một khung
		if err != nil {
			return "", err
		}
		back, _ := GetString(resp, "text")
		return back, nil
	}
	console := bufio.NewScanner(os.Stdin)
	for console.Scan() { // READ_CONSOLE()
		line := strings.TrimRight(console.Text(), "\r")
		back, err := exchange(line)
		if err != nil {
			fmt.Printf("[client] Lỗi: %v\n", err)
			return 1
		}
		fmt.Printf("[client] Gửi: '%s' -> Nhận: '%s'\n", line, back)
	}
	if big > 0 {
		text := makeText(big)
		fmt.Printf("[client] Gửi thông điệp lớn: %d ký tự, %d byte UTF-8\n", utf8.RuneCountInString(text), len(text))
		back, err := exchange(text)
		if err != nil {
			fmt.Printf("[client] Lỗi: %v\n", err)
			return 1
		}
		same := "không"
		if back == text {
			same = "có"
		}
		fmt.Printf("[client] Nhận lại: %d ký tự, %d byte; trùng khớp: %s\n", utf8.RuneCountInString(back), len(back), same)
		fmt.Printf("[client] SHA-256 gửi:  %s…\n", sha256Hex(text)[:32])
		fmt.Printf("[client] SHA-256 nhận: %s…\n", sha256Hex(back)[:32])
	}
	conn.(*net.TCPConn).CloseWrite() // SHUTDOWN(WRITE)
	return 0
}

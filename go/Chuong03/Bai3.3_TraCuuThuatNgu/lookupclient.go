package main

import (
	"bufio"
	"fmt"
	"os"
	"strings"
	"time"
)

// RunLookupClient – Mã giả 3.8 LOOKUP_CLIENT: giữ phần kết nối của ECHO_CLIENT (Mã giả 3.2),
// thay vòng gửi nhận thô bằng SEND_JSON/RECV_JSON (Mã giả 3.6).
// Mỗi dòng nhập là một thuật ngữ cần tra; hết dữ liệu nhập thì gửi {"op":"bye"}.
func RunLookupClient(args []string) int {
	host, port, err := ParseEndpoint(args, "127.0.0.1", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	conn, err := SafeConnect(host, port, 3000)
	if err != nil {
		fmt.Printf("[client] Không kết nối được %s:%d: %v\n", host, port, err)
		return 1
	}
	defer func() { // FINALLY CLOSE(s)
		conn.Close()
		fmt.Println("[client] Đã đóng kết nối")
	}()
	fmt.Printf("[client] Đã kết nối %s:%d. Nhập thuật ngữ cần tra (Ctrl+D để kết thúc).\n", host, port)
	console := bufio.NewScanner(os.Stdin)
	for console.Scan() { // WHILE (term ← READ_CONSOLE()) ≠ NULL
		term := strings.TrimRight(console.Text(), "\r")
		if err := SendJSON(conn, map[string]any{"op": "lookup", "term": term}); err != nil {
			fmt.Printf("[client] Lỗi: %v\n", err)
			return 1
		}
		conn.SetReadDeadline(time.Now().Add(5000 * time.Millisecond)) // RECV_TIMEOUT 5000 ms
		resp, err := RecvJSON(conn)
		if err != nil {
			fmt.Printf("[client] Lỗi: %v\n", err)
			return 1
		}
		if GetInt(resp, "status") == 200 {
			t, _ := GetString(resp, "term")
			text, _ := GetString(resp, "text")
			fmt.Printf("[client] %s: %s\n", t, text)
		} else {
			e, _ := GetString(resp, "error")
			fmt.Printf("[client] Lỗi %d: %s ('%s')\n", GetInt(resp, "status"), e, term)
		}
	}
	SendJSON(conn, map[string]any{"op": "bye"})
	return 0
}

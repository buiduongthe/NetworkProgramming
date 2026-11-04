package main

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"os"
	"strings"
	"time"
)

// Client tra cứu "bất đồng bộ" dùng khung JSON (minh họa Mã giả 5.1).
// Kế thừa Mã giả 3.8 (LOOKUP_CLIENT): đọc từng thuật ngữ từ bàn phím, gửi {op:"lookup"} và in phản hồi.
// Chương trình in thêm 4 byte tiêu đề của mỗi khung và minh họa việc hủy một lời gọi nhận đang chờ:
// sau khi hủy, trạng thái định khung không còn tin cậy nên kết nối bị đóng (Mục 5.4.1).

// RunFrameClient – chương trình client.
func RunFrameClient(args []string) int {
	host, port, err := ParseEndpoint(args, "127.0.0.1", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	ctx := context.Background()
	conn, err := SafeConnectAsync(ctx, host, port, 3000)
	if err != nil {
		fmt.Printf("[client] Không kết nối được: %v\n", err)
		return 1
	}
	defer func() {
		conn.Close() // CLOSE: không dùng tiếp kết nối đã hủy
		fmt.Println("[client] Đã đóng kết nối")
	}()
	fmt.Printf("[client] Đã kết nối %s:%d. Nhập thuật ngữ cần tra (Ctrl+D để kết thúc).\n", host, port)
	console := bufio.NewScanner(os.Stdin)
	for console.Scan() { // READ_CONSOLE()
		term := strings.TrimRight(console.Text(), "\r")
		req := map[string]any{"op": "lookup", "term": term}
		frame := MakeFrame(req)
		fmt.Printf("[client] Gửi khung %d byte, tiêu đề % X: %s\n", len(frame), frame[:4], JsonBytes(req))
		if err := SendJSONAsync(ctx, conn, req); err != nil {
			fmt.Printf("[client] Lỗi: %v\n", err)
			return 1
		}
		rctx, cancel := context.WithTimeout(ctx, 5000*time.Millisecond) // RECV_TIMEOUT 5000 ms
		resp, err := RecvJSONAsync(rctx, conn)
		cancel()
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
			fmt.Printf("[client] Lỗi %d: %s\n", GetInt(resp, "status"), e)
		}
	}
	// Minh họa hủy: chờ một khung mà server không gửi, tín hiệu hủy kích hoạt sau 500 ms
	cctx, cancel := context.WithCancel(ctx)
	time.AfterFunc(500*time.Millisecond, cancel) // CANCEL(ct) sau 500 ms
	_, err = RecvJSONAsync(cctx, conn)
	if errors.Is(err, context.Canceled) {
		fmt.Println("[client] RECV_JSON_ASYNC bị hủy sau 500 ms (Cancelled); trạng thái khung không xác định nên đóng kết nối")
	}
	return 0
}

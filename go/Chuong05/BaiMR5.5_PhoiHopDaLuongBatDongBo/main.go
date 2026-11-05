// Bài mở rộng MR5.5 – Phối hợp đa luồng và bất đồng bộ trong server mạng (Go).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1–5.6), Chương 6 (Bài 6.1, 6.2), MR5.4.
// Cùng kịch bản với bản Python (asyncio): vòng lặp sự kiện phải
//
//	(a) đẩy việc CPU (băm SHA-256) sang nhóm worker mà không chặn vòng lặp;
//	(b) gọi hàm thư viện cũ kiểu CHẶN an toàn (goroutine riêng);
//	(c) nhận kết quả từ một luồng HĐH chạy nền (cảm biến, runtime.LockOSThread) qua kênh;
//	(d) truyền hủy, hạn chót qua ranh giới (context.WithTimeout → worker select ctx.Done());
//	(e) áp lực ngược giữa vòng lặp và nhóm worker (đếm "suất" ở cổng vào + hàng đợi chờ);
//	(f) cách làm SAI – việc CPU ngay trên vòng lặp – đo bằng nhịp tim 10 ms.
//
//	out/app server <host> <port>
//	out/app tai    <host> <port>
package main

import (
	"fmt"
	"os"
)

func main() {
	a := os.Args
	if len(a) >= 4 && a[1] == "server" {
		runServer(a[2], a[3])
		return
	}
	if len(a) >= 4 && a[1] == "tai" {
		runTai(a[2], a[3])
		return
	}
	fmt.Println("Cách dùng: out/app server|tai <host> <port>")
	os.Exit(1)
}

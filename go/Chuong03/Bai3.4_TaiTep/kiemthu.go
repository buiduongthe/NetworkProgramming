package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net"
	"os"
	"strings"
	"time"
)

// Client kiểm thử chức năng tải tệp (ca 8, 9 của tập ca kiểm thử, Mục 3.5.3).
// Client tự tạo khung tiêu đề và nội dung (không dùng client chuẩn) để thử: tên tệp chứa "../"
// hoặc "..\", tên chỉ gồm thư mục, mã băm khai báo sai, kích thước vượt MAX_FILE và ngắt kết nối
// khi đang gửi nội dung. Mỗi ca mở một kết nối riêng tới server.

var testData = bytes.Repeat([]byte("Nội dung thử\n"), 10)

type kiemThu struct {
	host    string
	port    int
	results []bool
}

func (k *kiemThu) connect() net.Conn {
	conn, err := SafeConnect(k.host, k.port, 3000)
	if err != nil {
		fmt.Printf("[kiểm thử] Không kết nối được server: %v\n", err)
		os.Exit(1)
	}
	conn.SetDeadline(time.Now().Add(10 * time.Second))
	return conn
}

func (k *kiemThu) report(no, name string, ok bool, detail string) {
	k.results = append(k.results, ok)
	verdict := "KHÔNG ĐẠT"
	if ok {
		verdict = "ĐẠT"
	}
	fmt.Printf("[kiểm thử] Ca %s – %s: %s (%s)\n", no, name, verdict, detail)
}

func recv(conn net.Conn) map[string]any {
	r, err := RecvJSON(conn)
	if err != nil {
		fmt.Printf("[kiểm thử] Lỗi nhận: %v\n", err)
		return map[string]any{}
	}
	return r
}

func str(r map[string]any, key string) string {
	v, _ := GetString(r, key)
	return v
}

// putRaw gửi tiêu đề put; nếu server sẵn sàng (100) thì gửi nội dung và trả phản hồi cuối.
// digest rỗng → tự tính; size < 0 → dùng độ dài thật của data.
func putRaw(conn net.Conn, name string, data []byte, digest string, size int64) map[string]any {
	if digest == "" {
		sum := sha256.Sum256(data)
		digest = hex.EncodeToString(sum[:])
	}
	if size < 0 {
		size = int64(len(data))
	}
	SendJSON(conn, map[string]any{"op": "put", "name": name, "size": size, "sha256": digest})
	resp := recv(conn)
	if GetInt(resp, "status") != 100 {
		return resp
	}
	SendAll(conn, data)
	return recv(conn)
}

// RunKiemThu – chạy lần lượt các ca kiểm thử.
func RunKiemThu(args []string) int {
	host, port, err := ParseEndpoint(args, "127.0.0.1", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	k := &kiemThu{host: host, port: port}
	fmt.Printf("[kiểm thử] Server %s:%d\n", host, port)
	zeros := strings.Repeat("0", 64)

	s := k.connect()
	r := putRaw(s, "../thu.txt", testData, "", -1) // Ca 8a
	k.report("8a", "tên '../thu.txt'", GetInt(r, "status") == 200 && str(r, "saved") == "thu.txt",
		fmt.Sprintf("status %d, lưu '%s'", GetInt(r, "status"), str(r, "saved")))
	r = putRaw(s, `..\..\thu2.txt`, testData, "", -1) // Ca 8b
	k.report("8b", `tên '..\..\thu2.txt'`, GetInt(r, "status") == 200 && str(r, "saved") == "thu2.txt",
		fmt.Sprintf("status %d, lưu '%s'", GetInt(r, "status"), str(r, "saved")))
	r = putRaw(s, "../", testData, "", -1) // Ca 8c
	k.report("8c", "tên '../' (không còn tên tệp)", GetInt(r, "status") == 400,
		fmt.Sprintf("bị từ chối: status %d, %s", GetInt(r, "status"), str(r, "error")))
	r = putRaw(s, "sai_bam.txt", testData, zeros, -1) // Ca 9
	k.report("9", "mã băm khai báo sai", GetInt(r, "status") == 422,
		fmt.Sprintf("status %d, %s", GetInt(r, "status"), str(r, "error")))
	r = putRaw(s, "lon.bin", testData, "", 200*1048576) // Ca bổ sung 1
	k.report("bổ sung 1", "size 200 MiB > MAX_FILE", GetInt(r, "status") == 400,
		fmt.Sprintf("status %d, %s", GetInt(r, "status"), str(r, "error")))
	SendJSON(s, map[string]any{"op": "bye"})
	s.Close()

	s = k.connect() // Ca bổ sung 2: ngắt giữa chừng
	SendJSON(s, map[string]any{"op": "put", "name": "dang_do.txt", "size": 1000, "sha256": zeros})
	r = recv(s)
	SendAll(s, testData[:100]) // chỉ gửi 100/1000 byte rồi đóng
	s.Close()
	s = k.connect()
	SendJSON(s, map[string]any{"op": "list"})
	r2 := recv(s)
	k.report("bổ sung 2", "ngắt sau 100/1000 byte", GetInt(r, "status") == 100 && GetInt(r2, "status") == 200,
		fmt.Sprintf("server vẫn phục vụ client mới, list -> %d", GetInt(r2, "status")))
	SendJSON(s, map[string]any{"op": "bye"})
	s.Close()

	passed := 0
	for _, ok := range k.results {
		if ok {
			passed++
		}
	}
	fmt.Printf("[kiểm thử] Kết quả: %d/%d ca đạt\n", passed, len(k.results))
	return 0
}

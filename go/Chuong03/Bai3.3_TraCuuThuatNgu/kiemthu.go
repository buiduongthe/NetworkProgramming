package main

import (
	"encoding/binary"
	"fmt"
	"net"
	"os"
	"time"
)

// Client kiểm thử server tra cứu theo tập ca kiểm thử tối thiểu (Mục 3.5.3).
// Client này tự tạo dãy byte gửi đi (không dùng client chuẩn) để thực hiện các ca 1–7
// của bảng ca kiểm thử, cộng một ca bổ sung cho thao tác không có trong đặc tả.
// Mỗi ca mở một kết nối riêng tới server.

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
	conn.SetDeadline(time.Now().Add(10 * time.Second)) // thời gian chờ cho cả ca kiểm thử
	return conn
}

// frameOf – khung hoàn chỉnh (tiêu đề + nội dung) của một đối tượng JSON.
func frameOf(obj any) []byte {
	data, _ := JsonEncode(obj)
	return append(binary.BigEndian.AppendUint32(nil, uint32(len(data))), data...)
}

func (k *kiemThu) report(no, name string, ok bool, detail string) {
	k.results = append(k.results, ok)
	verdict := "KHÔNG ĐẠT"
	if ok {
		verdict = "ĐẠT"
	}
	fmt.Printf("[kiểm thử] Ca %s – %s: %s (%s)\n", no, name, verdict, detail)
}

// recv nhận một phản hồi JSON; lỗi được chuyển thành đối tượng rỗng để ca kiểm thử báo KHÔNG ĐẠT.
func recv(conn net.Conn) map[string]any {
	r, err := RecvJSON(conn)
	if err != nil {
		fmt.Printf("[kiểm thử] Lỗi nhận: %v\n", err)
		return map[string]any{}
	}
	return r
}

func lookup(conn net.Conn, term string) map[string]any {
	SendJSON(conn, map[string]any{"op": "lookup", "term": term})
	return recv(conn)
}

func str(r map[string]any, key string) string {
	v, _ := GetString(r, key)
	return v
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
	bye := map[string]any{"op": "bye"}

	s := k.connect() // Ca 1, 2
	r := lookup(s, "TCP")
	k.report("1", "thuật ngữ có trong từ điển", GetInt(r, "status") == 200 && str(r, "term") == "TCP",
		fmt.Sprintf("status %d", GetInt(r, "status")))
	r = lookup(s, "Bắt tay ba bước")
	k.report("2", "thuật ngữ có dấu", GetInt(r, "status") == 200 && str(r, "term") == "Bắt tay ba bước",
		fmt.Sprintf("status %d, term '%s'", GetInt(r, "status"), str(r, "term")))
	SendJSON(s, bye)
	s.Close()

	s = k.connect() // Ca 3: gửi từng byte, cách nhau 100 ms
	s.(*net.TCPConn).SetNoDelay(true)
	data := frameOf(map[string]any{"op": "lookup", "term": "UDP"})
	for i := range data {
		SendAll(s, data[i:i+1])
		time.Sleep(100 * time.Millisecond)
	}
	r = recv(s)
	k.report("3", fmt.Sprintf("gửi %d byte từng byte một", len(data)), GetInt(r, "status") == 200 && str(r, "term") == "UDP",
		fmt.Sprintf("status %d, term '%s'", GetInt(r, "status"), str(r, "term")))
	SendJSON(s, bye)
	s.Close()

	s = k.connect() // Ca 4: hai yêu cầu trong một lần SEND
	two := append(frameOf(map[string]any{"op": "lookup", "term": "TCP"}), frameOf(map[string]any{"op": "lookup", "term": "XYZ"})...)
	SendAll(s, two)
	r1, r2 := recv(s), recv(s)
	k.report("4", "hai yêu cầu trong một lần SEND", GetInt(r1, "status") == 200 && GetInt(r2, "status") == 404,
		fmt.Sprintf("status %d rồi %d", GetInt(r1, "status"), GetInt(r2, "status")))
	SendJSON(s, bye)
	s.Close()

	s = k.connect() // Ca 5: độ dài 0xFFFFFFFF > MAX_FRAME
	SendAll(s, []byte{0xff, 0xff, 0xff, 0xff})
	n, err := s.Read(make([]byte, 1)) // EOF hoặc RST đều nghĩa là server đã đóng
	closed := n == 0 && err != nil
	detail := "không đóng"
	if closed {
		detail = "server đóng kết nối"
	}
	k.report("5", "độ dài 4294967295 > MAX_FRAME", closed, detail)
	s.Close()

	s = k.connect() // Ca 6: nội dung không phải JSON
	SendFrame(s, []byte("đây không phải JSON"))
	r = recv(s)
	SendJSON(s, map[string]any{"op": "list"}) // kết nối vẫn dùng được
	r2 = recv(s)
	terms, _ := r2["terms"].([]any)
	k.report("6", "nội dung không phải JSON", GetInt(r, "status") == 400 && GetInt(r2, "status") == 200,
		fmt.Sprintf("status %d, sau đó list -> %d, %d thuật ngữ", GetInt(r, "status"), GetInt(r2, "status"), len(terms)))
	SendJSON(s, map[string]any{"op": "xoa", "term": "TCP"}) // Ca bổ sung: thao tác sai
	r = recv(s)
	k.report("bổ sung", `thao tác "xoa"`, GetInt(r, "status") == 400, fmt.Sprintf("status %d: %s", GetInt(r, "status"), str(r, "error")))
	SendJSON(s, bye)
	s.Close()

	s = k.connect() // Ca 7: đóng kết nối giữa khung
	SendAll(s, append(binary.BigEndian.AppendUint32(nil, 50), []byte("0123456789")...))
	s.Close()
	s = k.connect()
	r = lookup(s, "TCP")
	k.report("7", "đóng giữa khung (14/54 byte)", GetInt(r, "status") == 200,
		fmt.Sprintf("client mới vẫn được phục vụ, status %d", GetInt(r, "status")))
	SendJSON(s, bye)
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

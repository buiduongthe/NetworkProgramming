// Bài mở rộng MR4.2 – Tiện ích nhỏ: giải mã JSON, đọc trường, tách tham số dòng lệnh, xử lý Ctrl+C.
package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/signal"
	"strconv"
	"strings"
)

// Msg – một thông điệp JSON.
type Msg = map[string]any

// decodeJSON giải mã một datagram; trả nil nếu không phải đối tượng JSON hợp lệ.
// UseNumber: số giữ dạng văn bản để phân biệt số nguyên với 0.5 hay 1e3.
func decodeJSON(data []byte) Msg {
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.UseNumber()
	var obj Msg
	if dec.Decode(&obj) != nil {
		return nil
	}
	return obj
}

func str(m Msg, key string) string {
	s, _ := m[key].(string)
	return s
}

func num(m Msg, key string) int {
	v, _ := asInt(m[key])
	return v
}

// asInt: giá trị JSON có phải số nguyên không.
func asInt(v any) (int, bool) {
	if x, ok := v.(json.Number); ok {
		n, err := strconv.Atoi(x.String())
		return n, err == nil
	}
	return 0, false
}

// splitArgs tách tham số vị trí (host, port...) khỏi các cờ --ten giá_trị.
func splitArgs(args []string) (pos, flags []string) {
	for i, a := range args {
		if strings.HasPrefix(a, "-") {
			return pos, args[i:]
		}
		pos = append(pos, a)
	}
	return pos, nil
}

func argOr(pos []string, i int, def string) string {
	if i < len(pos) {
		return pos[i]
	}
	return def
}

// onInterrupt: khi nhấn Ctrl+C (SIGINT), in thông báo rồi kết thúc chương trình.
func onInterrupt(msg string) {
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		fmt.Println(msg)
		os.Exit(0)
	}()
}

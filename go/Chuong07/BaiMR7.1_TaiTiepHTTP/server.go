package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net/http"
	"os"
	"os/signal"
	"sync"
	"sync/atomic"
	"time"
)

// makeData – nội dung tệp cố định (giống bản Python) để hai bản cho cùng SHA-256.
func makeData(size int) []byte {
	b := make([]byte, size)
	for i := range b {
		b[i] = byte((i*31 + i/7) % 256)
	}
	return b
}

// cutWriter bọc ResponseWriter: ghi lại mã trạng thái, đếm byte và (khi được yêu cầu) ngắt
// kết nối sau limit byte bằng panic(http.ErrAbortHandler) – cách chuẩn để server net/http
// hủy phản hồi đang gửi và đóng kết nối mà không ghi nhật ký lỗi.
type cutWriter struct {
	http.ResponseWriter
	status  int
	written int
	limit   int // < 0: không ngắt
	onCut   func(status, n int)
}

func (c *cutWriter) WriteHeader(code int) { c.status = code; c.ResponseWriter.WriteHeader(code) }

func (c *cutWriter) Write(p []byte) (int, error) {
	if c.status == 0 {
		c.status = http.StatusOK
	}
	if c.limit >= 0 && c.written+len(p) >= c.limit {
		n, _ := c.ResponseWriter.Write(p[:c.limit-c.written])
		c.written += n
		http.NewResponseController(c.ResponseWriter).Flush() // đẩy phần đã ghi ra mạng trước khi ngắt
		c.onCut(c.status, c.written)
		panic(http.ErrAbortHandler)
	}
	n, err := c.ResponseWriter.Write(p)
	c.written += n
	return n, err
}

func runServer(addr string, size, cutAfter, cuts int) {
	data := makeData(size)
	sum := sha256.Sum256(data)
	sha := hex.EncodeToString(sum[:])
	etag := `"` + sha[:16] + `"`
	var counter atomic.Int64
	var mu sync.Mutex
	cutsLeft := cuts

	mux := http.NewServeMux()
	mux.HandleFunc("/tep.bin", func(w http.ResponseWriter, r *http.Request) {
		k := counter.Add(1)
		h := w.Header()
		h.Set("Content-Type", "application/octet-stream")
		h.Set("ETag", etag) // ServeContent so khớp If-Range với ETag này
		h.Set("Connection", "close")
		if r.Method == http.MethodHead {
			h.Set("X-Checksum-SHA256", sha)
		}
		rng := r.Header.Get("Range")
		if rng == "" {
			rng = "-"
		}
		cw := &cutWriter{ResponseWriter: w, limit: -1}
		mu.Lock()
		if r.Method == http.MethodGet && cutsLeft > 0 { // giả lập đứt kết nối giữa chừng
			cutsLeft--
			cw.limit = cutAfter
			cw.onCut = func(status, n int) {
				fmt.Printf("[server] #%d GET Range=%s -> %d, NGẮT sau %d byte\n", k, rng, status, n)
			}
		}
		mu.Unlock()
		// ServeContent xử lý sẵn Range (206, Content-Range), 416, If-Range và HEAD.
		http.ServeContent(cw, r, "tep.bin", time.Time{}, bytes.NewReader(data))
		switch {
		case r.Method == http.MethodHead:
			fmt.Printf("[server] #%d HEAD -> %d, %d byte\n", k, cw.status, size)
		case cw.status == http.StatusRequestedRangeNotSatisfiable:
			fmt.Printf("[server] #%d GET Range=%s -> 416\n", k, rng)
		default:
			fmt.Printf("[server] #%d GET Range=%s -> %d, gửi %d byte\n", k, rng, cw.status, cw.written)
		}
	})
	srv := &http.Server{Addr: addr, Handler: mux}
	go func() {
		ch := make(chan os.Signal, 1)
		signal.Notify(ch, os.Interrupt)
		<-ch
		fmt.Println("[server] Dừng server")
		srv.Close()
	}()
	fmt.Printf("[server] http://%s/tep.bin, %d byte, ETag %s\n", addr, size, etag)
	if err := srv.ListenAndServe(); err != http.ErrServerClosed {
		fmt.Println("[server] Lỗi:", err)
		os.Exit(1)
	}
}

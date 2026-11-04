package main

// Chương trình giao diện web cục bộ: phục vụ trang giaodien.html và API cho trình duyệt.
//   - mỗi tác vụ chạy trên một goroutine, xin lượt qua bộ giới hạn đồng thời, có context để hủy;
//   - tiến trình được đẩy tới trình duyệt bằng Server-Sent Events (SSE, Bài 7.4) qua /api/events;
//   - trình duyệt chỉ gọi API bằng fetch() bất đồng bộ nên luồng chính của trang không bao giờ chờ;
//   - /api/bad: handler làm việc tải tệp ngay trong lúc trả lời (chặn ~1,2 s). Server Go KHÔNG bị
//     đứng (mỗi request một goroutine), nhưng trang gọi nó bằng XMLHttpRequest ĐỒNG BỘ nên luồng
//     chính của trình duyệt bị chặn – tương tự "làm sai trên luồng giao diện" ở bản Tkinter.

import (
	"context"
	_ "embed"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"sync"
	"syscall"
	"time"
)

//go:embed giaodien.html
var pageHTML []byte

type fileSpec struct{ kb, delay int }

var files = []fileSpec{{256, 60}, {256, 90}, {384, 70}, {256, 120}, {320, 50}, {256, 100}, {192, 80}, {256, 40}}
var names = []string{"router.lab", "may-in.lab", "camera-1.lab", "nas.lab", "khong-co.lab"}

// ---------- trạm phát sự kiện SSE ----------
type hub struct {
	mu   sync.Mutex
	subs map[chan []byte]bool
}

func (h *hub) subscribe() chan []byte {
	c := make(chan []byte, 1024)
	h.mu.Lock()
	h.subs[c] = true
	h.mu.Unlock()
	return c
}

func (h *hub) unsubscribe(c chan []byte) {
	h.mu.Lock()
	delete(h.subs, c)
	h.mu.Unlock()
}

func (h *hub) publish(ev map[string]any) {
	b, _ := json.Marshal(ev)
	h.mu.Lock()
	defer h.mu.Unlock()
	for c := range h.subs {
		select {
		case c <- b:
		default: // trình duyệt quá chậm: bỏ sự kiện thay vì chặn goroutine tác vụ
		}
	}
}

// ---------- quản lý tác vụ ----------
type task struct {
	id     int
	name   string
	cancel context.CancelFunc
	done   bool
}

type manager struct {
	mu    sync.Mutex
	tasks map[int]*task
	next  int
	lim   *limiter
	hub   *hub
	test  string // địa chỉ máy chủ thử host:port
	t0    time.Time
}

func (m *manager) log(text string) {
	line := fmt.Sprintf("%6.2f s  %s", time.Since(m.t0).Seconds(), text)
	outMu.Lock()
	fmt.Println("[ui] " + line)
	outMu.Unlock()
	m.hub.publish(map[string]any{"type": "log", "text": line})
}

type job func(ctx context.Context, progress progressFn) (string, error)

func (m *manager) spawn(name string, j job) {
	ctx, cancel := context.WithCancel(context.Background())
	m.mu.Lock()
	m.next++
	t := &task{id: m.next, name: name, cancel: cancel}
	m.tasks[t.id] = t
	m.mu.Unlock()
	m.hub.publish(map[string]any{"type": "new", "id": t.id, "name": name})
	ticket := m.lim.enter() // xếp hàng ngay, theo thứ tự tạo tác vụ
	go func() {
		defer cancel()
		finish := func(text string) {
			m.mu.Lock()
			t.done = true
			m.mu.Unlock()
			m.hub.publish(map[string]any{"type": "done", "id": t.id, "text": text})
			m.log(fmt.Sprintf("%s: %s", name, text))
		}
		if !m.lim.wait(ctx, ticket) {
			finish("đã hủy (khi đang chờ lượt)")
			return
		}
		defer m.lim.release()
		m.hub.publish(map[string]any{"type": "state", "id": t.id, "text": "đang chạy…"})
		res, err := j(ctx, func(pct int, text string) {
			m.hub.publish(map[string]any{"type": "progress", "id": t.id, "pct": pct, "text": text})
		})
		switch {
		case ctx.Err() != nil:
			finish("đã hủy")
		case err != nil:
			finish("lỗi: " + err.Error())
		default:
			finish(res)
		}
	}()
}

func (m *manager) running() []*task {
	m.mu.Lock()
	defer m.mu.Unlock()
	var r []*task
	for i := 1; i <= m.next; i++ {
		if t, ok := m.tasks[i]; ok && !t.done {
			r = append(r, t)
		}
	}
	return r
}

// ---------- các handler HTTP ----------
func (m *manager) routes() *http.ServeMux {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /{$}", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		w.Write(pageHTML)
	})
	mux.HandleFunc("GET /api/events", m.events)
	mux.HandleFunc("POST /api/start", func(w http.ResponseWriter, r *http.Request) {
		host, portS, _ := net.SplitHostPort(m.test)
		port, _ := strconv.Atoi(portS)
		switch r.URL.Query().Get("kind") {
		case "tai":
			n, _ := strconv.Atoi(r.URL.Query().Get("n"))
			n = max(1, min(n, len(files)))
			m.log(fmt.Sprintf("Bấm Tải tệp: %d tệp, giới hạn đồng thời %d", n, m.lim.get()))
			for _, f := range files[:n] {
				m.spawn(fmt.Sprintf("Tải %d KB (%d ms/khối)", f.kb, f.delay), func(ctx context.Context, p progressFn) (string, error) {
					return download(ctx, m.test, f.kb, f.delay, p)
				})
			}
		case "cong":
			ports := []int{port, port + 1, port + 2, port + 3}
			m.log(fmt.Sprintf("Bấm Kiểm tra cổng: %d, %d, %d, %d", ports[0], ports[1], ports[2], ports[3]))
			m.spawn("Kiểm tra 4 cổng", func(ctx context.Context, p progressFn) (string, error) {
				return checkPorts(ctx, host, ports, p)
			})
		case "dns":
			m.log(fmt.Sprintf("Bấm Tra cứu tên: %d tên qua UDP", len(names)))
			m.spawn(fmt.Sprintf("Tra cứu %d tên", len(names)), func(ctx context.Context, p progressFn) (string, error) {
				return lookupNames(ctx, m.test, names, p)
			})
		}
		w.WriteHeader(http.StatusNoContent)
	})
	mux.HandleFunc("POST /api/cancel", func(w http.ResponseWriter, r *http.Request) {
		id, _ := strconv.Atoi(r.URL.Query().Get("id"))
		m.mu.Lock()
		t := m.tasks[id]
		m.mu.Unlock()
		if t != nil {
			m.log(fmt.Sprintf("Bấm Hủy ở tác vụ #%d", id))
			t.cancel()
		}
		w.WriteHeader(http.StatusNoContent)
	})
	mux.HandleFunc("POST /api/cancel_all", func(w http.ResponseWriter, r *http.Request) {
		run := m.running()
		m.log(fmt.Sprintf("Bấm Hủy tất cả: %d tác vụ", len(run)))
		for _, t := range run {
			t.cancel()
		}
		w.WriteHeader(http.StatusNoContent)
	})
	mux.HandleFunc("POST /api/limit", func(w http.ResponseWriter, r *http.Request) {
		n, _ := strconv.Atoi(r.URL.Query().Get("n"))
		m.lim.set(max(1, n))
		m.log(fmt.Sprintf("Đổi giới hạn đồng thời: %d", max(1, n)))
		w.WriteHeader(http.StatusNoContent)
	})
	mux.HandleFunc("POST /api/log", func(w http.ResponseWriter, r *http.Request) {
		m.log(r.URL.Query().Get("text")) // trang gửi số đo của nó để in vào nhật ký chung
		w.WriteHeader(http.StatusNoContent)
	})
	mux.HandleFunc("POST /api/bad", func(w http.ResponseWriter, r *http.Request) {
		m.log("Bấm Làm SAI: trang gọi /api/bad ĐỒNG BỘ, handler tải 256 KB rồi mới trả lời")
		t := time.Now()
		_, err := download(context.Background(), m.test, 256, 80, func(int, string) {})
		ms := time.Since(t).Milliseconds()
		m.log(fmt.Sprintf("/api/bad trả lời sau %d ms (các goroutine khác vẫn chạy bình thường)", ms))
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{"ms": ms, "ok": err == nil})
	})
	return mux
}

// events – luồng SSE: mỗi sự kiện một dòng "data: <json>", kết thúc bằng dòng trống.
func (m *manager) events(w http.ResponseWriter, r *http.Request) {
	fl, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "không hỗ trợ streaming", http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	c := m.hub.subscribe()
	defer m.hub.unsubscribe(c)
	fmt.Fprint(w, ": kết nối SSE\n\n")
	fl.Flush()
	for {
		select {
		case b := <-c:
			fmt.Fprintf(w, "data: %s\n\n", b)
			fl.Flush()
		case <-r.Context().Done():
			return
		}
	}
}

func runUI(host string, port int, test string) {
	m := &manager{tasks: map[int]*task{}, lim: newLimiter(3), hub: &hub{subs: map[chan []byte]bool{}},
		test: test, t0: time.Now()}
	addr := net.JoinHostPort(host, strconv.Itoa(port))
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		fmt.Println("[ui] lỗi:", err)
		os.Exit(1)
	}
	m.log(fmt.Sprintf("Giao diện: http://%s/ (máy chủ thử %s)", addr, test))
	go func() {
		sig := make(chan os.Signal, 1)
		signal.Notify(sig, os.Interrupt, syscall.SIGTERM)
		<-sig
		m.log("dừng")
		os.Exit(0)
	}()
	http.Serve(ln, m.routes())
}

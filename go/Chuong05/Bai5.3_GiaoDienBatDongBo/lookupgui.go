package main

import (
	"context"
	_ "embed"
	"errors"
	"fmt"
	"net"
	"net/http"
	"strconv"
	"sync"
)

// Ứng dụng tra cứu có giao diện – Mã giả 5.3 (cách 1: luồng nền và UI_INVOKE) cho nút Kết nối và
// Mã giả 5.4 (cách 2: trình xử lý sự kiện bất đồng bộ) cho nút Tra cứu (Mục 5.3.2).
// Kế thừa: Mã giả 2.2 (SAFE_CONNECT), 3.8 (một lượt tra cứu), 5.1 (khung JSON bất đồng bộ).
//
// Thư viện chuẩn Go không có bộ công cụ giao diện đồ họa, nên giao diện là một trang web cục bộ:
//   - Trình duyệt giữ vai trò luồng giao diện: mọi mã JavaScript trong giaodien.html chạy trên một luồng
//     duy nhất theo vòng lặp sự kiện, giống vòng lặp thông điệp của WinForms, Swing, Tkinter.
//   - Chương trình Go giữ kết nối TCP tới server tra cứu (conn, ct là trường của "cửa sổ")
//     và thực hiện mọi thao tác mạng trong goroutine.
//   - UI_INVOKE: goroutine nền đưa (thủ tục, tham số) vào hàng đợi uiQueue; trang web nhận hàng đợi
//     này qua Server-Sent Events (SSE) và thực hiện thủ tục trên luồng giao diện.

//go:embed giaodien.html
var pageHTML []byte

// uiCall – một yêu cầu UI_INVOKE: tên thủ tục giao diện và tham số.
type uiCall struct {
	Proc string `json:"proc"`
	Args []any  `json:"args"`
}

// LookupWindow – trạng thái của "cửa sổ": kết nối, tín hiệu hủy, hàng đợi UI_INVOKE.
type LookupWindow struct {
	mu      sync.Mutex
	conn    net.Conn           // conn: kết nối tới server tra cứu
	ctx     context.Context    // ct: tín hiệu hủy, kích hoạt khi đóng cửa sổ
	cancel  context.CancelFunc //
	uiQueue chan uiCall        // hàng đợi UI_INVOKE
}

// UiInvoke – UI_INVOKE(proc, args): gọi được từ goroutine nền, không chạm trực tiếp vào giao diện.
func (w *LookupWindow) UiInvoke(proc string, args ...any) {
	fmt.Printf("[gui]   UI_INVOKE(%s, %s)\n", proc, JsonBytes(args))
	w.uiQueue <- uiCall{proc, args}
}

// handleEvents – luồng SSE: chuyển từng yêu cầu UI_INVOKE tới trang web.
func (w *LookupWindow) handleEvents(rw http.ResponseWriter, r *http.Request) {
	rw.Header().Set("Content-Type", "text/event-stream")
	rw.Header().Set("Cache-Control", "no-cache")
	flusher, _ := rw.(http.Flusher)
	flusher.Flush()
	for {
		select {
		case call := <-w.uiQueue:
			fmt.Fprintf(rw, "data: %s\n\n", JsonBytes(call))
			flusher.Flush()
		case <-r.Context().Done():
			return
		}
	}
}

// onConnectClick – phần Go của Mã giả 5.3 ON_CONNECT_CLICK: trang web đã tắt nút Kết nối,
// ở đây chỉ khởi động goroutine nền rồi trả lời ngay (SPAWN_THREAD(CONNECT_WORKER, …)).
func (w *LookupWindow) onConnectClick(rw http.ResponseWriter, r *http.Request) {
	host := r.FormValue("host")
	port, err := strconv.Atoi(r.FormValue("port")) // TO_INT(txtPort.text)
	if err != nil || port < 1 || port > 65535 {
		fmt.Printf("[gui] Bấm Kết nối: cổng không hợp lệ '%s'\n", r.FormValue("port"))
		w.UiInvoke("showStatus", "Cổng không hợp lệ: "+r.FormValue("port"))
		w.UiInvoke("setEnabled", "btnConnect", true)
		rw.WriteHeader(http.StatusAccepted)
		return
	}
	fmt.Printf("[gui] Bấm Kết nối %s:%d: giao cho goroutine nền\n", host, port)
	go w.connectWorker(host, port) // SPAWN_THREAD: không chờ
	rw.WriteHeader(http.StatusAccepted)
}

// connectWorker – Mã giả 5.3 CONNECT_WORKER(host, port): chạy trên goroutine nền.
func (w *LookupWindow) connectWorker(host string, port int) {
	conn, err := SafeConnectAsync(w.ctx, host, port, 3000) // SAFE_CONNECT: chờ trên goroutine nền, không chặn giao diện
	var ne *NetError
	if err == nil {
		w.mu.Lock()
		w.conn = conn
		w.mu.Unlock()
		fmt.Printf("[gui] CONNECT_WORKER: đã kết nối %s\n", conn.RemoteAddr())
		w.UiInvoke("showStatus", "Đã kết nối")
		w.UiInvoke("setEnabled", "btnLookup", true)
		return
	}
	kind := err.Error()
	if errors.As(err, &ne) { // CATCH NetError AS e
		kind = ne.Kind
	}
	fmt.Printf("[gui] CONNECT_WORKER: lỗi %s\n", kind)
	w.UiInvoke("showStatus", "Lỗi: "+kind)
	w.UiInvoke("setEnabled", "btnConnect", true)
}

// onLookupClick – phần Go của Mã giả 5.4: trình xử lý async trên trang web gọi tới đây và AWAIT kết quả;
// mỗi yêu cầu HTTP được net/http phục vụ trên một goroutine riêng.
func (w *LookupWindow) onLookupClick(rw http.ResponseWriter, r *http.Request) {
	w.mu.Lock() // một kết nối, một yêu cầu tại một thời điểm (giữ đúng định khung)
	defer w.mu.Unlock()
	rw.Header().Set("Content-Type", "application/json; charset=utf-8")
	fail := func(err error) { // NetError, ProtocolError → trang web hiện "Lỗi: …"
		fmt.Printf("[gui] Tra cứu lỗi: %v\n", err)
		rw.WriteHeader(http.StatusBadGateway)
		rw.Write(JsonBytes(map[string]any{"loi": err.Error()}))
	}
	if w.conn == nil {
		fail(errors.New("chưa kết nối"))
		return
	}
	req := map[string]any{"op": "lookup", "term": r.FormValue("term")}
	fmt.Printf("[gui] Bấm Tra cứu: gửi %s\n", JsonBytes(req))
	if err := SendJSONAsync(w.ctx, w.conn, req); err != nil { // AWAIT SEND_JSON_ASYNC(conn, req, ct)
		fail(err)
		return
	}
	resp, err := RecvJSONAsync(w.ctx, w.conn) // AWAIT RECV_JSON_ASYNC(conn, ct)
	if err != nil {
		fail(err)
		return
	}
	fmt.Printf("[gui] Nhận phản hồi: %s\n", JsonBytes(resp))
	rw.Write(JsonBytes(resp))
}

// onClose – đóng cửa sổ: hủy mọi thao tác đang chờ (CANCEL(ct)) và đóng kết nối.
func (w *LookupWindow) onClose(rw http.ResponseWriter, r *http.Request) {
	fmt.Println("[gui] Cửa sổ đóng: hủy ct, đóng kết nối")
	w.cancel()
	w.mu.Lock()
	if w.conn != nil {
		w.conn.Close()
		w.conn = nil
	}
	w.mu.Unlock()
}

// RunLookupGui – gui [host] [port] [cổng_giao_diện]: mở giao diện web tại http://127.0.0.1:<cổng_giao_diện>/.
func RunLookupGui(args []string) int {
	host, port, err := ParseEndpoint(args[:min(len(args), 2)], "127.0.0.1", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	uiPort := 8080
	if len(args) > 2 {
		if uiPort, err = strconv.Atoi(args[2]); err != nil || uiPort < 1 || uiPort > 65535 {
			fmt.Printf("Cổng không hợp lệ: %s\n", args[2])
			return 2
		}
	}
	w := &LookupWindow{uiQueue: make(chan uiCall, 100)}
	w.ctx, w.cancel = context.WithCancel(context.Background())
	mux := http.NewServeMux()
	mux.HandleFunc("GET /{$}", func(rw http.ResponseWriter, r *http.Request) {
		rw.Header().Set("Content-Type", "text/html; charset=utf-8")
		rw.Write(pageHTML)
	})
	mux.HandleFunc("GET /events", w.handleEvents)
	mux.HandleFunc("POST /api/connect", w.onConnectClick)
	mux.HandleFunc("POST /api/lookup", w.onLookupClick)
	mux.HandleFunc("POST /api/close", w.onClose)
	ln, err := net.Listen("tcp4", net.JoinHostPort("127.0.0.1", strconv.Itoa(uiPort))) // chỉ máy cục bộ mở được giao diện
	if err != nil {
		fmt.Printf("[gui] Không mở được cổng giao diện: %v\n", err)
		return 1
	}
	fmt.Printf("[gui] Mở trình duyệt tại http://127.0.0.1:%d/?host=%s&port=%d\n", uiPort, host, port)
	StopOnCtrlC(ln)
	http.Serve(ln, mux)
	fmt.Println("[gui] Dừng giao diện")
	return 0
}

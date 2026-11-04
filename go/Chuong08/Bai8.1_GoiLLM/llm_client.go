// Bài 8.1 – Gọi API LLM có truyền luồng SSE, giới hạn tần suất, thử lại và hạn chót.
//
// Cài đặt Mã giả 8.2 – CALL_LLM (Mục 8.2.3, Giáo trình Lập trình mạng), kế thừa
// Mã giả 8.1 – STREAM_COMPLETION (llm_stream.go), Mã giả 5.8 – RETRY_ASYNC và
// Mã giả 8.3 – ACQUIRE_TOKENS (thung_token.go).
// Cấu hình qua biến môi trường: LLM_BASE_URL (mặc định http://127.0.0.1:9000/v1 – server giả lập),
// LLM_API_KEY (bắt buộc), LLM_MODEL (mặc định mock-model), LLM_PRICE_IN, LLM_PRICE_OUT
// (đơn giá USD cho 1 triệu token, tùy chọn).
//
// Chạy:  python3 mock_llm_server.py 127.0.0.1 9000 --fail-first 1           (cửa sổ 1)
//
//	LLM_API_KEY=khoa-thu out/app "So sánh TCP và UDP."                  (cửa sổ 2)
package main

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"math/rand/v2"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"
	"unicode/utf8"
)

var BASE_URL = strings.TrimRight(envOr("LLM_BASE_URL", "http://127.0.0.1:9000/v1"), "/")
var MODEL = envOr("LLM_MODEL", "mock-model")

var bucket = newBucket(20000, 2000) // hạn mức tự đặt: 20 000 token, nạp 2 000 token/giây

func envOr(name, def string) string {
	if v := os.Getenv(name); v != "" {
		return v
	}
	return def
}

// secret – SECRET(name): bí mật chỉ lấy từ biến môi trường, không ghi trong mã nguồn, không in ra.
func secret(name string) (string, error) {
	if v := os.Getenv(name); v != "" {
		return v, nil
	}
	return "", errors.New("Thiếu biến môi trường " + name)
}

type Message struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

type ChatRequest struct {
	Model     string    `json:"model"`
	Stream    bool      `json:"stream"`
	MaxTokens int       `json:"max_tokens"`
	Messages  []Message `json:"messages"`
}

// estimateTokens – ESTIMATE_TOKENS: ước lượng thô ~4 ký tự/token cho đầu vào, cộng max_tokens cho đầu ra.
func estimateTokens(req ChatRequest) float64 {
	chars := 0
	for _, m := range req.Messages {
		chars += utf8.RuneCountInString(m.Content)
	}
	return float64(chars/4 + req.MaxTokens)
}

// parseRetryAfter – PARSE_RETRY_AFTER: Retry-After là số giây hoặc ngày giờ HTTP (RFC 9110); trả về ms, -1 nếu không có.
func parseRetryAfter(h http.Header) int {
	v := strings.TrimSpace(h.Get("Retry-After"))
	if v == "" {
		return -1
	}
	if s, err := strconv.Atoi(v); err == nil {
		return s * 1000
	}
	if t, err := http.ParseTime(v); err == nil {
		return max(0, int(time.Until(t).Milliseconds()))
	}
	return -1
}

// httpClient: thời gian chờ kết nối 3 s, chờ tiêu đề phản hồi tối đa timeoutMs.
func httpClient(timeoutMs int) *http.Client {
	return &http.Client{Transport: &http.Transport{
		DialContext:           (&net.Dialer{Timeout: 3 * time.Second}).DialContext,
		ResponseHeaderTimeout: time.Duration(timeoutMs) * time.Millisecond,
	}}
}

// TimeoutError – Timeout của mã giả: hết thời gian chờ kết nối hoặc chờ phản hồi (RETRY_ASYNC thử lại).
type TimeoutError struct{ msg string }

func (e *TimeoutError) Error() string { return e.msg }

// httpsPost – HTTPS_POST(url, req, hdrs, timeout_ms, ct): net/http tự bọc TLS khi URL là https://;
// trả về phản hồi (thân chưa đọc) và hàm giải phóng. Yêu cầu gắn với ct (hủy được); thân được bọc
// idleReader: im lặng quá timeoutMs giữa hai lần nhận thì hủy yêu cầu.
func httpsPost(url string, req ChatRequest, hdrs map[string]string, timeoutMs int, ct context.Context) (*http.Response, *bufio.Reader, func(), error) {
	data, _ := json.Marshal(req)
	ctx, cancel := context.WithCancel(ct)
	rq, _ := http.NewRequestWithContext(ctx, "POST", url, bytes.NewReader(data))
	for k, v := range hdrs {
		rq.Header.Set(k, v)
	}
	rq.Header.Set("Content-Type", "application/json")
	rq.Header.Set("Accept", "text/event-stream")
	resp, err := httpClient(timeoutMs).Do(rq) // 4xx, 5xx vẫn là phản hồi (err = nil)
	if err != nil {
		cancel()
		var ne net.Error
		switch {
		case ct.Err() != nil: // Cancelled: người dùng hủy, không thử lại
			return nil, nil, nil, ct.Err()
		case errors.Is(err, syscall.ECONNREFUSED):
			return nil, nil, nil, &NetError{msg: "REFUSED", retry: false, retryAfter: -1}
		case errors.As(err, &ne) && ne.Timeout():
			return nil, nil, nil, &TimeoutError{"Hết thời gian chờ (" + err.Error() + ")"}
		}
		return nil, nil, nil, &NetError{msg: "UNREACHABLE (" + err.Error() + ")", retry: true, retryAfter: -1}
	}
	idle := time.Duration(timeoutMs) * time.Millisecond
	ir := &idleReader{r: resp.Body, timer: time.AfterFunc(idle, cancel), idle: idle}
	closeAll := func() { ir.timer.Stop(); resp.Body.Close(); cancel() }
	return resp, bufio.NewReaderSize(ir, 65536+2), closeAll, nil
}

// retryCall – Mã giả 5.8, RETRY_ASYNC(op, max_tries, base_ms, cap_ms, ct).
func retryCall(op func(context.Context) (*StreamState, error), maxTries int, baseMs, capMs float64, ct context.Context) (*StreamState, error) {
	for k := 0; ; k++ {
		st, err := op(ct) // AWAIT op(ct)
		if err == nil {
			return st, nil
		}
		var ne *NetError
		var te *TimeoutError
		isTimeout := errors.As(err, &te)
		isNet := errors.As(err, &ne)
		if !isTimeout && !isNet { // CATCH Timeout, NetError; Cancelled và lỗi khác: không bắt
			return nil, err
		}
		ok := isTimeout || ne.retry // ok ← (e IS Timeout) OR e.retry
		if !ok || k == maxTries-1 {
			return nil, err
		}
		delay := rand.Float64() * math.Min(capMs, baseMs*math.Pow(2, float64(k))) // jitter
		if isNet && ne.retryAfter >= 0 {
			delay = math.Max(delay, float64(ne.retryAfter)) // server đề nghị
		}
		fmt.Printf("[llm] Lần thử %d/%d: %v – chờ %.0f ms rồi thử lại\n", k+1, maxTries, err, delay)
		select { // AWAIT DELAY_ASYNC(delay, ct): hủy được khi đang chờ
		case <-time.After(time.Duration(delay * float64(time.Millisecond))):
		case <-ct.Done():
			return nil, ct.Err()
		}
	}
}

// callLLM – Mã giả 8.2, CALL_LLM(req, on_token, ct).
func callLLM(req ChatRequest, onToken func(string), ct context.Context) (*StreamState, error) {
	started := false
	oneAttempt := func(ct context.Context) (*StreamState, error) { // ONE_ATTEMPT(ct)
		if err := acquireTokens(bucket, estimateTokens(req)); err != nil { // 8.2.4
			return nil, err
		}
		key, err := secret("LLM_API_KEY")
		if err != nil {
			return nil, err
		}
		hdrs := map[string]string{"Authorization": "Bearer " + key}
		resp, r, closeAll, err := httpsPost(BASE_URL+"/chat/completions", req, hdrs, 30000, ct)
		if err != nil {
			return nil, err
		}
		defer closeAll()
		switch s := resp.StatusCode; {
		case s == 408 || s == 429 || s == 500 || s == 502 || s == 503 || s == 504:
			return nil, &NetError{msg: fmt.Sprintf("HTTP %d", s), retry: !started, retryAfter: parseRetryAfter(resp.Header)}
		case s >= 400:
			return nil, &NetError{msg: fmt.Sprintf("HTTP %d", s), retry: false, retryAfter: -1}
		}
		tok := func(d string) { started = true; onToken(d) }
		st, err := streamCompletion(r, tok)
		if err != nil && ct.Err() != nil { // luồng dừng vì người dùng hủy, không phải do mạng
			return nil, ct.Err()
		}
		return st, err
	}
	return retryCall(oneAttempt, 5, 500, 20000, ct)
}

func main() {
	question := strings.Join(os.Args[1:], " ")
	if question == "" {
		question = "So sánh TCP và UDP."
	}
	req := ChatRequest{Model: MODEL, Stream: true, MaxTokens: 512, Messages: []Message{
		{"system", "Trợ lý mạng, trả lời ngắn."}, {"user", question}}}
	fmt.Printf("[llm] Mô hình %s tại %s\n", MODEL, BASE_URL)
	fmt.Printf("[llm] Câu hỏi: %s\n", question)
	if _, err := secret("LLM_API_KEY"); err != nil {
		fmt.Printf("[llm] %v\n", err)
		os.Exit(2)
	}
	t0 := time.Now()
	var first time.Time
	onToken := func(d string) { // UI: UI_INVOKE(APPEND, d)
		if first.IsZero() {
			first = time.Now()
			fmt.Print("[llm] Trả lời: ")
		}
		fmt.Print(d)
	}
	ct, stop := signal.NotifyContext(context.Background(), os.Interrupt) // ct: Ctrl+C hủy lời gọi
	defer stop()
	st, err := callLLM(req, onToken, ct)
	if errors.Is(err, context.Canceled) {
		fmt.Println("\n[llm] Đã hủy lời gọi (Ctrl+C)")
		os.Exit(130)
	}
	if err != nil {
		note := ""
		if !first.IsZero() {
			fmt.Println()
			note = " – không thử lại vì đã hiển thị một phần câu trả lời"
		}
		fmt.Printf("[llm] Lỗi: %v%s\n", err, note)
		os.Exit(1)
	}
	total := time.Now()
	fmt.Println()
	fmt.Printf("[llm] TTFT: %d ms, tổng thời gian: %d ms\n", first.Sub(t0).Milliseconds(), total.Sub(t0).Milliseconds())
	u := Usage{}
	if st.usage != nil {
		u = *st.usage
	}
	fmt.Printf("[llm] usage: prompt_tokens = %d, completion_tokens = %d\n", u.PromptTokens, u.CompletionTokens)
	pIn, pOut := os.Getenv("LLM_PRICE_IN"), os.Getenv("LLM_PRICE_OUT")
	if pIn != "" && pOut != "" { // Công thức (8.1): N_in·p_in + N_out·p_out
		fin, err1 := strconv.ParseFloat(pIn, 64)
		fout, err2 := strconv.ParseFloat(pOut, 64)
		if err1 == nil && err2 == nil {
			cost := (float64(u.PromptTokens)*fin + float64(u.CompletionTokens)*fout) / 1_000_000
			fmt.Printf("[llm] Chi phí ước tính: %d × %s + %d × %s (USD/1 triệu token) = %.8f USD\n",
				u.PromptTokens, pIn, u.CompletionTokens, pOut, cost)
		}
	}
}

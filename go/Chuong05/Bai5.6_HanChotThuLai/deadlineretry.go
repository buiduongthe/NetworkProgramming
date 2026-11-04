package main

import (
	"context"
	"errors"
	"fmt"
	"math/rand"
	"net"
	"strconv"
	"time"
)

// Yêu cầu có hạn chót và thử lại có lùi theo cấp số nhân + nhiễu ngẫu nhiên –
// Mã giả 5.7 REQUEST_WITH_DEADLINE (Mục 5.4.2) và Mã giả 5.8 RETRY_ASYNC (Mục 5.4.3).
// Kế thừa: Mã giả 5.1 (khung JSON bất đồng bộ), 2.2 (SAFE_CONNECT, NetError có trường retry),
// 4.4 (UDP_REQUEST – vòng gửi lại có lùi thời gian).
// Ánh xạ Go: LINK(user_ct, CANCEL_AFTER(timeout_ms)) ↔ context.WithTimeout(userCtx, timeout):
// ngữ cảnh con bị hủy khi ngữ cảnh cha bị hủy (người dùng bấm Hủy) hoặc khi hết hạn.

// TimeoutError – Timeout: hết hạn chót của toàn bộ yêu cầu.
type TimeoutError struct{ Msg string }

func (e *TimeoutError) Error() string { return e.Msg }

// RequestWithDeadline – Mã giả 5.7 REQUEST_WITH_DEADLINE(sock, req, timeout_ms, user_ct).
func RequestWithDeadline(userCtx context.Context, sock net.Conn, req map[string]any, timeoutMs int) (map[string]any, error) {
	ctx, cancel := context.WithTimeout(userCtx, time.Duration(timeoutMs)*time.Millisecond) // ct ← LINK(user_ct, CANCEL_AFTER(timeout_ms))
	defer cancel()
	err := SendJSONAsync(ctx, sock, req) // AWAIT SEND_JSON_ASYNC(sock, req, ct)
	var resp map[string]any
	if err == nil {
		resp, err = RecvJSONAsync(ctx, sock) // AWAIT RECV_JSON_ASYNC(sock, ct)
	}
	if err != nil && ctx.Err() != nil { // CATCH Cancelled
		sock.Close() // trạng thái khung không xác định
		if userCtx.Err() != nil {
			return nil, context.Canceled // IF user_ct.cancelled THEN RAISE Cancelled
		}
		return nil, &TimeoutError{fmt.Sprintf("Quá hạn %d ms", timeoutMs)} // RAISE Timeout
	}
	return resp, err
}

// RetryAsync – Mã giả 5.8 RETRY_ASYNC(op, max_tries, base_ms, cap_ms, ct).
func RetryAsync(ctx context.Context, op func(ctx context.Context) (map[string]any, error), maxTries, baseMs, capMs int) (map[string]any, error) {
	if maxTries < 1 {
		return nil, errors.New("max_tries phải ≥ 1")
	}
	for k := 0; k < maxTries; k++ { // FOR k ← 0 TO max_tries - 1
		resp, err := op(ctx) // RETURN AWAIT op(ct)
		if err == nil {
			return resp, nil
		}
		var te *TimeoutError
		var ne *NetError
		isTimeout, isNet := errors.As(err, &te), errors.As(err, &ne)
		if !isTimeout && !isNet { // Cancelled và lỗi khác: không bắt
			return nil, err
		}
		ok := isTimeout || ne.Retry // ok ← (e IS Timeout) OR e.retry
		if !ok || k == maxTries-1 {
			return nil, err // RAISE e
		}
		upper := min(capMs, baseMs<<k)           // MIN(cap_ms, base_ms * 2^k)
		delay := rand.Float64() * float64(upper) // RANDOM(0, …): jitter
		kind := "TIMEOUT"
		if isNet {
			kind = ne.Kind
			if ne.RetryAfterMs > 0 { // IF e.retry_after ≠ NULL
				delay = max(delay, float64(ne.RetryAfterMs)) // server đề nghị
			}
		}
		log(fmt.Sprintf("  lỗi %s (có thể thử lại); chờ %.0f ms, giới hạn trên %d ms", kind, delay, upper))
		select { // AWAIT DELAY_ASYNC(delay, ct): hủy được khi đang chờ
		case <-time.After(time.Duration(delay * float64(time.Millisecond))):
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}
	return nil, errors.New("không đến được đây")
}

var t0 = time.Now()

func log(text string) {
	fmt.Printf("[client] [t=%5d ms] %s\n", time.Since(t0).Milliseconds(), text)
}

// makeOp tạo hàm op(ct): mỗi lần gọi là một lần thử mới (kết nối mới, yêu cầu mới).
func makeOp(host string, port int, req map[string]any, deadlineMs int) func(ctx context.Context) (map[string]any, error) {
	tries := 0
	return func(ctx context.Context) (map[string]any, error) {
		tries++
		log(fmt.Sprintf("Lần thử %d: gửi %s", tries, JsonBytes(req)))
		sock, err := SafeConnectAsync(ctx, host, port, 3000)
		if err != nil {
			return nil, err
		}
		resp, err := RequestWithDeadline(ctx, sock, req, deadlineMs)
		if err != nil {
			return nil, err // Timeout, Cancelled hoặc lỗi mạng
		}
		sock.Close()
		if GetInt(resp, "status") == 503 { // quá tải tạm thời: lỗi có thể thử lại
			return nil, &NetError{Kind: "UNAVAILABLE", Retry: true, RetryAfterMs: GetInt(resp, "retry_after_ms")}
		}
		return resp, nil
	}
}

// scenario chạy một kịch bản; cancelMs > 0 mô phỏng người dùng bấm Hủy sau cancelMs.
func scenario(title string, run func(ctx context.Context) (map[string]any, error), cancelMs int) {
	t0 = time.Now()
	fmt.Printf("[client] === %s ===\n", title)
	ctx, cancel := context.WithCancel(context.Background()) // user_ct
	defer cancel()
	if cancelMs > 0 {
		time.AfterFunc(time.Duration(cancelMs)*time.Millisecond, cancel) // nút Hủy
	}
	resp, err := run(ctx)
	var ne *NetError
	var te *TimeoutError
	switch {
	case err == nil:
		log("Thành công: " + string(JsonBytes(resp)))
	case errors.Is(err, context.Canceled):
		log("Người dùng đã hủy (Cancelled) – không thử lại")
	case errors.As(err, &ne):
		log(fmt.Sprintf("Thất bại, không thử lại: %s (retry=%t)", ne.Kind, ne.Retry))
	case errors.As(err, &te):
		log("Thất bại sau khi hết lượt: " + te.Msg)
	default:
		log(fmt.Sprintf("Lỗi: %v", err))
	}
}

// RunDeadlineRetry – ba kịch bản của bài.
func RunDeadlineRetry(args []string) int {
	host, port, err := ParseEndpoint(args[:min(len(args), 2)], "127.0.0.1", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	bad := 9101
	if len(args) > 2 {
		if bad, err = strconv.Atoi(args[2]); err != nil || bad < 1 || bad > 65535 {
			fmt.Printf("Cổng không hợp lệ: %s\n", args[2])
			return 2
		}
	}
	scenario("Kịch bản 1: hạn chót 1000 ms, thử lại tối đa 5 lần (base 200, cap 5000)", func(ctx context.Context) (map[string]any, error) {
		return RetryAsync(ctx, makeOp(host, port, map[string]any{"op": "lookup", "term": "TCP"}, 1000), 5, 200, 5000)
	}, 0)
	scenario("Kịch bản 2: server xử lý 3000 ms, hạn chót 5000 ms, người dùng bấm Hủy sau 500 ms", func(ctx context.Context) (map[string]any, error) {
		return RetryAsync(ctx, makeOp(host, port, map[string]any{"op": "lookup", "term": "UDP", "delay_ms": 3000}, 5000), 5, 200, 5000)
	}, 500)
	scenario(fmt.Sprintf("Kịch bản 3: cổng %d không có server (REFUSED)", bad), func(ctx context.Context) (map[string]any, error) {
		return RetryAsync(ctx, makeOp(host, bad, map[string]any{"op": "lookup", "term": "TCP"}, 1000), 5, 200, 5000)
	}, 0)
	return 0
}

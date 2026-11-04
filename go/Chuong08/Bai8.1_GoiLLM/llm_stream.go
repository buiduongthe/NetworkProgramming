// Bài 8.1 – Nhận câu trả lời của LLM theo luồng SSE.
//
// Cài đặt Mã giả 8.1 – STREAM_COMPLETION (Mục 8.2.2, Giáo trình Lập trình mạng),
// kế thừa Mã giả 7.5 – SSE_READ (Mục 7.3.3) và lỗi NetError của Mã giả 2.2.
// Tệp này được llm_client.go dùng.
package main

import (
	"bufio"
	"encoding/json"
	"errors"
	"io"
	"strings"
	"time"
)

// NetError – NetError của Mã giả 2.2: mang thông điệp, cờ có thể thử lại và Retry-After (ms, -1 = không có).
type NetError struct {
	msg        string
	retry      bool
	retryAfter int
}

func (e *NetError) Error() string { return e.msg }

// idleReader: đóng vai thời gian chờ nhàn rỗi giữa hai lần nhận dữ liệu – mỗi lần đọc được dữ liệu
// thì đặt lại bộ hẹn giờ; hết giờ mà không có dữ liệu thì hàm onIdle hủy yêu cầu HTTP.
type idleReader struct {
	r     io.Reader
	timer *time.Timer
	idle  time.Duration
}

func (ir *idleReader) Read(p []byte) (int, error) {
	n, err := ir.r.Read(p)
	ir.timer.Reset(ir.idle)
	return n, err
}

// readLine – READ_LINE (Mã giả 3.5) trên thân phản hồi; net/http đã giải mã khối (chunked).
// Trả về ok = false khi thân kết thúc hoặc kết nối bị ngắt (SSE_READ chỉ dừng ở hai trường hợp này).
func readLine(r *bufio.Reader, maxLen int) (string, bool, error) {
	line, err := r.ReadSlice('\n')
	if errors.Is(err, bufio.ErrBufferFull) || len(line) > maxLen {
		return "", false, &NetError{msg: "Dòng SSE quá dài"} // ProtocolError("Dòng quá dài")
	}
	if err != nil { // io.EOF, ngắt kết nối, khối bị cắt dở, hết thời gian chờ
		return "", false, nil
	}
	return strings.TrimRight(string(line), "\r\n"), true, nil
}

// sseRead – Mã giả 7.5, SSE_READ(r, on_event); onEvent trả lỗi thì dừng đọc.
func sseRead(r *bufio.Reader, onEvent func(typ, data string) error) error {
	evType, data := "message", []string{}
	for {
		line, ok, err := readLine(r, 65536)
		if err != nil {
			return err
		}
		if !ok {
			return nil // NULL: server đóng luồng
		}
		if line == "" { // dòng trống: hết một sự kiện
			if len(data) > 0 {
				if err := onEvent(evType, strings.Join(data, "\n")); err != nil {
					return err
				}
			}
			evType, data = "message", []string{}
		} else if !strings.HasPrefix(line, ":") { // ":": dòng chú thích
			field, value, _ := strings.Cut(line, ":")
			value = strings.TrimPrefix(value, " ") // STRIP_ONE_SPACE
			switch field {
			case "event":
				evType = value
			case "data":
				data = append(data, value)
				// "id", "retry": không dùng trong bài này
			}
		}
	}
}

// Usage: số token đầu vào, đầu ra do server báo.
type Usage struct {
	PromptTokens     int `json:"prompt_tokens"`
	CompletionTokens int `json:"completion_tokens"`
}

// chunk: một sự kiện của luồng Chat Completions; Content là con trỏ để phân biệt "không có" với "".
type chunk struct {
	Usage   *Usage `json:"usage"`
	Choices []struct {
		Delta struct {
			Content *string `json:"content"`
		} `json:"delta"`
	} `json:"choices"`
}

// StreamState – st ← {text, usage, done} của Mã giả 8.1.
type StreamState struct {
	text  string
	usage *Usage
	done  bool
}

// streamCompletion – Mã giả 8.1, STREAM_COMPLETION(r, on_token); r: thân phản hồi sau tiêu đề.
func streamCompletion(r *bufio.Reader, onToken func(string)) (*StreamState, error) {
	st := &StreamState{}                           // dòng 1
	onEvent := func(_ string, data string) error { // dòng 2
		if data == "[DONE]" { // dòng 3: không phải JSON
			st.done = true
			return nil
		}
		var c chunk
		if err := json.Unmarshal([]byte(data), &c); err != nil { // dòng 4
			return &NetError{msg: "Sự kiện SSE không phải JSON"}
		}
		if c.Usage != nil { // dòng 5–6
			st.usage = c.Usage
		}
		if len(c.Choices) == 0 { // dòng 7: sự kiện chỉ mang usage
			return nil
		}
		if d := c.Choices[0].Delta.Content; d != nil { // dòng 8–12
			st.text += *d
			onToken(*d)
		}
		return nil
	}
	if err := sseRead(r, onEvent); err != nil { // dòng 14
		return nil, err
	}
	if !st.done { // dòng 15
		return nil, &NetError{msg: "Luồng bị ngắt giữa chừng"}
	}
	return st, nil // dòng 16
}

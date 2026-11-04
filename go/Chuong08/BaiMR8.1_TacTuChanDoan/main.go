// Bài mở rộng MR8.1 – Tác tử AI chẩn đoán mạng bằng gọi công cụ (tool calling), có lớp kiểm soát.
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 8.1 (gọi API LLM) và Bài 8.3 (MCP server):
// Bài 8.3 viết phía server công cụ, bài này viết phía "chủ" (host/agent) – vòng lặp
// hỏi LLM -> LLM yêu cầu công cụ -> chương trình KIỂM TRA rồi mới chạy -> gửi kết quả -> ...
// cho tới khi LLM trả lời bằng văn bản. Lớp kiểm soát (tools.go): danh mục công cụ, lược đồ
// tham số, chính sách đích (chỉ loopback và mạng riêng), giới hạn số vòng, nhật ký kiểm toán.
// Cấu hình qua biến môi trường: LLM_BASE_URL, LLM_API_KEY, LLM_MODEL (như Bài 8.1).
// LLM giả lập: mock_llm_cong_cu.py (Python, thư viện chuẩn) – không gọi dịch vụ LLM thật.
//
// Chạy:  out/app "<câu hỏi>"
package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"strings"
	"time"
)

const maxRounds = 4

type toolCall struct {
	ID       string `json:"id"`
	Type     string `json:"type"`
	Function struct {
		Name      string `json:"name"`
		Arguments string `json:"arguments"`
	} `json:"function"`
}

// message – một tin nhắn hội thoại; Content là con trỏ để mã hóa được null như API OpenAI.
type message struct {
	Role       string     `json:"role"`
	Content    *string    `json:"content"`
	ToolCalls  []toolCall `json:"tool_calls,omitempty"`
	ToolCallID string     `json:"tool_call_id,omitempty"`
}

func str(s string) *string { return &s }

func getenv(k, def string) string {
	if v := os.Getenv(k); v != "" {
		return v
	}
	return def
}

// chat – một lần gọi POST /chat/completions (không truyền luồng), gửi kèm danh mục công cụ.
func chat(msgs []message) (message, error) {
	key := os.Getenv("LLM_API_KEY")
	if key == "" {
		fmt.Println("Thiếu biến môi trường LLM_API_KEY")
		os.Exit(1)
	}
	var toolList []any
	for _, name := range []string{"phan_giai_ten", "kiem_tra_cong"} {
		t := tools[name]
		toolList = append(toolList, map[string]any{"type": "function",
			"function": map[string]any{"name": name, "description": t.Description, "parameters": t.Schema}})
	}
	body, _ := json.Marshal(map[string]any{"model": getenv("LLM_MODEL", "mock-model"),
		"messages": msgs, "temperature": 0, "tools": toolList})
	url := strings.TrimRight(getenv("LLM_BASE_URL", "http://127.0.0.1:9000/v1"), "/") + "/chat/completions"
	req, _ := http.NewRequest(http.MethodPost, url, bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+key)
	resp, err := (&http.Client{Timeout: 10 * time.Second}).Do(req)
	if err != nil {
		return message{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return message{}, fmt.Errorf("HTTP %d", resp.StatusCode)
	}
	var out struct {
		Choices []struct {
			Message message `json:"message"`
		} `json:"choices"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil || len(out.Choices) == 0 {
		return message{}, fmt.Errorf("phản hồi không hợp lệ: %v", err)
	}
	return out.Choices[0].Message, nil
}

// wrap – ngắt dòng theo từ, mỗi dòng tối đa width ký tự (như textwrap.wrap của Python).
func wrap(text string, width int) []string {
	var lines []string
	cur := ""
	for _, w := range strings.Fields(text) {
		if cur != "" && len([]rune(cur))+1+len([]rune(w)) > width {
			lines = append(lines, cur)
			cur = w
		} else if cur == "" {
			cur = w
		} else {
			cur += " " + w
		}
	}
	if cur != "" {
		lines = append(lines, cur)
	}
	return lines
}

// compact – JSON gọn; map được json.Marshal sắp khóa theo thứ tự chữ cái.
func compact(v any) string {
	var b bytes.Buffer
	enc := json.NewEncoder(&b)
	enc.SetEscapeHTML(false)
	enc.Encode(v)
	return strings.TrimSpace(b.String())
}

func main() {
	question := "Kiểm tra localhost cổng 80"
	if len(os.Args) > 1 {
		question = os.Args[1]
	}
	fmt.Println("[tac-tu] Hỏi:", question)
	msgs := []message{
		{Role: "system", Content: str("Bạn là trợ lý chẩn đoán mạng. Chỉ dùng công cụ được cung cấp.")},
		{Role: "user", Content: str(question)},
	}
	for rnd := 1; rnd <= maxRounds; rnd++ {
		msg, err := chat(msgs)
		if err != nil {
			fmt.Println("[tac-tu] Lỗi gọi LLM:", err)
			os.Exit(1)
		}
		msgs = append(msgs, msg)
		if len(msg.ToolCalls) == 0 {
			fmt.Printf("[tac-tu] Vòng %d: LLM trả lời\n", rnd)
			if msg.Content != nil {
				for _, l := range wrap(*msg.Content, 58) {
					fmt.Println("  " + l)
				}
			}
			return
		}
		fmt.Printf("[tac-tu] Vòng %d: LLM yêu cầu %d công cụ\n", rnd, len(msg.ToolCalls))
		for _, c := range msg.ToolCalls {
			name, args := c.Function.Name, c.Function.Arguments
			result, err := runTool(name, args)
			if err != nil {
				result = map[string]any{"loi": err.Error()}
				fmt.Printf("  CHẶN  %s%s: %v\n", name, args, err)
			} else {
				fmt.Printf("  CHẠY  %s%s -> %s\n", name, args, compact(result))
			}
			msgs = append(msgs, message{Role: "tool", ToolCallID: c.ID, Content: str(compact(result))})
		}
	}
	fmt.Printf("[tac-tu] Dừng: quá %d vòng mà chưa có câu trả lời\n", maxRounds)
}

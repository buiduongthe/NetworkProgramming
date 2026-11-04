// Bài mở rộng MR8.2 – Kiểm duyệt cấu hình do LLM sinh ra trước khi dùng (guardrail + vòng phản hồi).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 8.1 (gọi API LLM), Bài 3.3 (JSON) và
// Bài MR1.1 (kiểm tra địa chỉ). Đầu ra của LLM là DỮ LIỆU CHƯA TIN CẬY: tách JSON, kiểm tra
// lược đồ, ngữ nghĩa (VLAN, mạng con, cổng mặc định, chồng lấn, hạ tầng đang dùng) và chính
// sách an toàn (validate.go). Có lỗi thì gửi danh sách lỗi lại cho LLM để sửa, tối đa N lần;
// hết lượt vẫn sai thì TỪ CHỐI. Cấu hình đạt mới được chuyển thành lệnh CLI (chờ người duyệt).
// Cấu hình qua biến môi trường: LLM_BASE_URL, LLM_API_KEY, LLM_MODEL (như Bài 8.1).
// LLM giả lập: mock_llm_cau_hinh.py (Python, thư viện chuẩn) – không gọi dịch vụ LLM thật.
//
// Chạy:  out/app <ha_tang.json> "<yêu cầu>" [số_lần]      (mặc định 3)
package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"
)

const systemPrompt = "Bạn sinh cấu hình mạng. Chỉ trả về một đối tượng JSON có các khóa: " +
	"vlan (danh sách {id, ten, mang, cong_mac_dinh}), acl (danh sách {ten, quy_tac}), " +
	"dich_vu ({telnet, ssh})."

type message struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

func getenv(k, def string) string {
	if v := os.Getenv(k); v != "" {
		return v
	}
	return def
}

// chat – một lần gọi POST /chat/completions, yêu cầu định dạng JSON.
func chat(msgs []message) (string, error) {
	key := os.Getenv("LLM_API_KEY")
	if key == "" {
		fmt.Println("Thiếu biến môi trường LLM_API_KEY")
		os.Exit(1)
	}
	body, _ := json.Marshal(map[string]any{"model": getenv("LLM_MODEL", "mock-model"), "messages": msgs,
		"temperature": 0, "response_format": map[string]string{"type": "json_object"}})
	url := strings.TrimRight(getenv("LLM_BASE_URL", "http://127.0.0.1:9000/v1"), "/") + "/chat/completions"
	req, _ := http.NewRequest(http.MethodPost, url, bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+key)
	resp, err := (&http.Client{Timeout: 10 * time.Second}).Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("HTTP %d", resp.StatusCode)
	}
	var out struct {
		Choices []struct {
			Message message `json:"message"`
		} `json:"choices"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil || len(out.Choices) == 0 {
		return "", fmt.Errorf("phản hồi không hợp lệ: %v", err)
	}
	return out.Choices[0].Message.Content, nil
}

func main() {
	if len(os.Args) < 3 {
		fmt.Println(`Cách dùng: out/app <ha_tang.json> "<yêu cầu>" [số_lần]`)
		os.Exit(1)
	}
	data, err := os.ReadFile(os.Args[1])
	var infra Infra
	if err == nil {
		err = json.Unmarshal(data, &infra)
	}
	if err != nil {
		fmt.Println("[kiem-duyet] Lỗi đọc hạ tầng:", err)
		os.Exit(1)
	}
	request := os.Args[2]
	tries := 3
	if len(os.Args) > 3 {
		tries, _ = strconv.Atoi(os.Args[3])
	}
	msgs := []message{{"system", systemPrompt}, {"user", request}}
	fmt.Println("[kiem-duyet] Yêu cầu:", request)
	for attempt := 1; attempt <= tries; attempt++ {
		text, err := chat(msgs)
		if err != nil {
			fmt.Println("[kiem-duyet] Lỗi gọi LLM:", err)
			os.Exit(1)
		}
		msgs = append(msgs, message{"assistant", text})
		cfg, errs := validate(text, infra)
		if len(errs) == 0 {
			fmt.Printf("[kiem-duyet] Lần %d: ĐẠT\n", attempt)
			fmt.Println("[kiem-duyet] Lệnh CLI sinh từ cấu hình đã duyệt (chờ người duyệt):")
			for _, l := range render(cfg) {
				fmt.Println("  " + l)
			}
			return
		}
		fmt.Printf("[kiem-duyet] Lần %d: %d lỗi\n", attempt, len(errs))
		var fb strings.Builder
		fb.WriteString("Cấu hình chưa đạt, hãy sửa và chỉ trả về JSON:")
		for _, e := range errs {
			fmt.Println("  - " + e)
			fb.WriteString("\n- " + e)
		}
		msgs = append(msgs, message{"user", fb.String()})
	}
	fmt.Printf("[kiem-duyet] TỪ CHỐI: sau %d lần vẫn chưa đạt, không sinh lệnh\n", tries)
	os.Exit(1)
}

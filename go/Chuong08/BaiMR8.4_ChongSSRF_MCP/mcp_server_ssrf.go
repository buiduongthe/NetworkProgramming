// Bài mở rộng MR8.4 – MCP server có công cụ fetch_url, so sánh bản NGÂY THƠ và bản AN TOÀN.
//
// Nối tiếp Bài 8.3 (MCP server công cụ mạng qua stdio, JSON-RPC 2.0 – giữ nguyên khung thông điệp).
// Bài mở rộng, không có trong giáo trình. Hai công cụ: fetch_url_naive, fetch_url_safe.
// JSON-RPC 2.0, mỗi thông điệp một dòng UTF-8 trên stdin/stdout; nhật ký chỉ ghi ra stderr.
//
// Chạy (do host khởi chạy như tiến trình con):  out/app
package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"slices"
	"strings"
)

var supported = []string{"2026-07-28"}

// Tool – mô tả công cụ kèm lược đồ JSON Schema của đầu vào.
type Tool struct {
	Name        string `json:"name"`
	Description string `json:"description"`
	InputSchema struct {
		Type       string                       `json:"type"`
		Properties map[string]map[string]string `json:"properties"`
		Required   []string                     `json:"required"`
	} `json:"inputSchema"`
}

func tool(name, description string, params ...string) Tool {
	t := Tool{Name: name, Description: description}
	t.InputSchema.Type = "object"
	t.InputSchema.Properties = map[string]map[string]string{}
	for i := 0; i+1 < len(params); i += 2 {
		t.InputSchema.Properties[params[i]] = map[string]string{"type": params[i+1]}
		t.InputSchema.Required = append(t.InputSchema.Required, params[i])
	}
	return t
}

var tools = []Tool{
	tool("fetch_url_naive", "Tải URL KHÔNG kiểm tra (minh họa lỗ hổng SSRF)", "url", "string"),
	tool("fetch_url_safe", "Tải URL có kiểm soát chống SSRF", "url", "string"),
}

type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

func (e *rpcError) Error() string { return e.Message }

type resultMsg struct {
	Jsonrpc string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Result  any             `json:"result"`
}

type errorMsg struct {
	Jsonrpc string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Error   rpcError        `json:"error"`
}

type content struct {
	Type string `json:"type"`
	Text string `json:"text"`
}

type callResult struct {
	ResultType string    `json:"resultType"`
	IsError    bool      `json:"isError"`
	Content    []content `json:"content"`
}

func jsonEncode(obj any) []byte {
	var b bytes.Buffer
	enc := json.NewEncoder(&b)
	enc.SetEscapeHTML(false)
	enc.Encode(obj)
	return b.Bytes()
}

func writeMsg(obj any) { os.Stdout.Write(jsonEncode(obj)) }

func readLine(r *bufio.Reader, maxLen int) (string, error) {
	line, err := r.ReadSlice('\n')
	if errors.Is(err, bufio.ErrBufferFull) || len(line) > maxLen {
		return "", errors.New("Dòng quá dài")
	}
	if err != nil && len(line) == 0 {
		return "", err
	}
	return strings.TrimRight(string(line), "\r\n"), nil
}

func decodeObject(raw json.RawMessage) map[string]any {
	var m map[string]any
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.UseNumber()
	if dec.Decode(&m) != nil || m == nil {
		return map[string]any{}
	}
	return m
}

func ok(text string) callResult  { return callResult{"complete", false, []content{{"text", text}}} }
func bad(text string) callResult { return callResult{"complete", true, []content{{"text", text}}} }

func tomTatThan(body []byte) string {
	text := strings.TrimSpace(strings.ReplaceAll(string(body), "\n", " "))
	r := []rune(text)
	if len(r) > 120 {
		return string(r[:120]) + "…"
	}
	return text
}

func callTool(nameV any, a map[string]any, _ string) (callResult, error) {
	name, isStr := nameV.(string)
	if !isStr || !slices.ContainsFunc(tools, func(t Tool) bool { return t.Name == name }) {
		if !isStr {
			name = "null"
		}
		return callResult{}, &rpcError{-32602, "Unknown tool: " + name}
	}
	url, isStr := a["url"].(string)
	if !isStr {
		return bad("Tham số 'url' phải là chuỗi"), nil
	}
	logErr("gọi", name, url)
	var (
		status int
		body   []byte
		steps  []string
		err    error
	)
	if name == "fetch_url_naive" {
		status, body, steps, err = fetchNaive(url)
	} else {
		status, body, steps, err = fetchSafe(url)
	}
	if err != nil {
		logErr("   TỪ CHỐI/LỖI:", err)
		return bad(err.Error()), nil
	}
	for _, s := range steps {
		logErr("   ", s)
	}
	tail := steps
	if len(steps) > 2 {
		tail = steps[len(steps)-2:]
	}
	text := fmt.Sprintf("HTTP %d | %s | thân: %s", status, strings.Join(tail, " | "), tomTatThan(body))
	return ok(text), nil
}

func dispatch(method string, rawParams json.RawMessage) (any, error) {
	p := decodeObject(rawParams)
	meta, _ := p["_meta"].(map[string]any)
	if meta == nil {
		return nil, &rpcError{-32602, "Invalid params: thiếu _meta"}
	}
	v, _ := meta["io.modelcontextprotocol/protocolVersion"].(string)
	_, hasCaps := meta["io.modelcontextprotocol/clientCapabilities"]
	if v == "" || !hasCaps {
		return nil, &rpcError{-32602, "Invalid params: thiếu _meta"}
	}
	if !slices.Contains(supported, v) {
		return nil, &rpcError{-32022, "UnsupportedProtocolVersion"}
	}
	switch method {
	case "server/discover":
		return map[string]any{"resultType": "complete", "supportedVersions": supported,
			"capabilities": map[string]any{"tools": map[string]any{}},
			"serverInfo":   map[string]any{"name": "ssrf-guard-mcp", "version": "1.0.0"}}, nil
	case "tools/list":
		return map[string]any{"resultType": "complete", "tools": tools,
			"ttlMs": 3600000, "cacheScope": "public"}, nil
	case "tools/call":
		name := p["name"]
		args, _ := p["arguments"].(map[string]any)
		if args == nil {
			args = map[string]any{}
		}
		return callTool(name, args, "")
	}
	return nil, &rpcError{-32601, "Method not found"}
}

func main() {
	logErr("MCP server chống SSRF (stdio), phiên bản", strings.Join(supported, ", "))
	r := bufio.NewReaderSize(os.Stdin, 1<<20)
	for {
		line, err := readLine(r, 1<<20)
		if err != nil {
			break
		}
		var raw map[string]json.RawMessage
		if json.Unmarshal([]byte(line), &raw) != nil {
			writeMsg(errorMsg{"2.0", nil, rpcError{-32700, "Parse error"}})
			continue
		}
		idRaw, hasID := raw["id"]
		var method string
		if m, ok := raw["method"]; ok {
			json.Unmarshal(m, &method)
		}
		if !hasID || string(idRaw) == "null" {
			logErr("<- thông báo", method, ": không trả lời")
			continue
		}
		logErr("<- id="+string(idRaw), method)
		res, derr := dispatch(method, raw["params"])
		if derr != nil {
			var re *rpcError
			if errors.As(derr, &re) {
				writeMsg(errorMsg{"2.0", idRaw, *re})
			} else {
				writeMsg(errorMsg{"2.0", idRaw, rpcError{-32603, "Internal error"}})
			}
			continue
		}
		writeMsg(resultMsg{"2.0", idRaw, res})
	}
	logErr("Host đã đóng stdin, kết thúc")
}

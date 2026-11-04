// Bài 8.3 – MCP server công cụ mạng qua kênh stdio (đặc tả MCP 2026-07-28 như Mục 8.3 trình bày).
//
// Cài đặt Mã giả 8.4 (MCP_STDIO_SERVER, DISPATCH, WRITE_MSG), Mã giả 8.5 (CALL_TOOL) và dùng
// Mã giả 8.7 (kiểm soát đích) trong net_tools.go; Mục 8.3.5, 8.5.4 Giáo trình Lập trình mạng.
// JSON-RPC 2.0, mỗi thông điệp một dòng UTF-8 trên stdin/stdout; nhật ký chỉ ghi ra stderr.
// Biến môi trường ALLOWED_NETS: các mạng được phép (CIDR, phân cách dấu phẩy), ví dụ 10.10.0.0/16.
//
// Chạy (do host khởi chạy như tiến trình con):  ALLOWED_NETS=127.0.0.0/8 out/app
// Thử nhanh:  python3 mcp_test_client.py 9000 9001 out/app
package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"slices"
	"strings"
)

var SUPPORTED = []string{"2026-07-28"} // dòng 1

// Tool – TOOL(tên, mô tả, tham số) -> mô tả công cụ kèm lược đồ JSON Schema của đầu vào.
type Tool struct {
	Name        string `json:"name"`
	Description string `json:"description"`
	InputSchema struct {
		Type       string                       `json:"type"`
		Properties map[string]map[string]string `json:"properties"` // map: khóa được xếp theo bảng chữ cái
		Required   []string                     `json:"required"`
	} `json:"inputSchema"`
}

func tool(name, description string, params ...string) Tool { // params: tên1, kiểu1, tên2, kiểu2, …
	t := Tool{Name: name, Description: description}
	t.InputSchema.Type = "object"
	t.InputSchema.Properties = map[string]map[string]string{}
	for i := 0; i+1 < len(params); i += 2 {
		t.InputSchema.Properties[params[i]] = map[string]string{"type": params[i+1]}
		t.InputSchema.Required = append(t.InputSchema.Required, params[i])
	}
	return t
}

var TOOLS = []Tool{ // dòng 2–6
	tool("resolve_host", "Phân giải tên miền", "host", "string"),
	tool("check_port", "Kiểm tra cổng TCP", "host", "string", "port", "integer"),
	tool("ping", "Đo thời gian khứ hồi ICMP", "host", "string"),
}

// RpcError: lỗi JSON-RPC (mã, thông điệp).
type RpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

func (e *RpcError) Error() string { return e.Message }

type resultMsg struct {
	Jsonrpc string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Result  any             `json:"result"`
}

type errorMsg struct {
	Jsonrpc string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"` // nil → null
	Error   RpcError        `json:"error"`
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

// jsonEncode – JSON_ENCODE gọn, UTF-8, không thoát <, >, &.
func jsonEncode(obj any) []byte {
	var b bytes.Buffer
	enc := json.NewEncoder(&b)
	enc.SetEscapeHTML(false)
	enc.Encode(obj) // Encode thêm '\n' ở cuối: đúng một thông điệp một dòng
	return b.Bytes()
}

// writeMsg – WRITE_MSG: một thông điệp một dòng; os.Stdout của Go không có bộ đệm nên không cần FLUSH.
func writeMsg(obj any) {
	os.Stdout.Write(jsonEncode(obj))
}

// readLine – READ_LINE (Mã giả 3.5) trên stdin; io.EOF (NULL) khi host đóng stdin.
func readLine(r *bufio.Reader, maxLen int) (string, error) {
	line, err := r.ReadSlice('\n')
	if errors.Is(err, bufio.ErrBufferFull) || len(line) > maxLen {
		return "", errors.New("Dòng quá dài") // ProtocolError
	}
	if err != nil && len(line) == 0 {
		return "", err
	}
	return strings.TrimRight(string(line), "\r\n"), nil
}

// decodeObject: giải mã một đối tượng JSON, số giữ dạng json.Number (phân biệt 22 với 22.0).
func decodeObject(raw json.RawMessage) map[string]any {
	var m map[string]any
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.UseNumber()
	if dec.Decode(&m) != nil || m == nil {
		return map[string]any{}
	}
	return m
}

func ok(text string) callResult {
	return callResult{"complete", false, []content{{"text", text}}}
}

// callTool – Mã giả 8.5, CALL_TOOL(name, a): kiểm tra tham số trước khi thực thi công cụ.
func callTool(nameV any, a map[string]any, rawArgs string) (callResult, error) {
	name, isStr := nameV.(string)
	if !isStr || !slices.ContainsFunc(TOOLS, func(t Tool) bool { return t.Name == name }) {
		if !isStr {
			name = "null" // tên công cụ không phải chuỗi
		}
		return callResult{}, &RpcError{-32602, "Unknown tool: " + name}
	}
	logErr("  gọi công cụ", name, rawArgs)
	res, err := func() (callResult, error) { // TRY
		switch name {
		case "resolve_host":
			host, err := validateName(a["host"])
			if err != nil {
				return callResult{}, err
			}
			var parts []string // TO_TEXT
			for _, x := range resolveHost(host) {
				parts = append(parts, familyName(x)+" "+x.String())
			}
			if len(parts) == 0 {
				return ok("Không có địa chỉ nào"), nil
			}
			return ok(strings.Join(parts, "; ")), nil
		case "check_port":
			ip, err := validateTarget(a["host"])
			if err != nil {
				return callResult{}, err
			}
			port, err := validatePort(a["port"])
			if err != nil {
				return callResult{}, err
			}
			return ok(fmt.Sprintf("%s:%d %s", ip, port, portCheck(ip, port, 2000))), nil
		default: // "ping"
			ip, err := validateTarget(a["host"])
			if err != nil {
				return callResult{}, err
			}
			addr, rtt, ttl, err := ping(ip, 1, 2000)
			if err != nil {
				return callResult{}, err
			}
			return ok(fmt.Sprintf("%s: thời gian khứ hồi %.3f ms, TTL %d", addr, rtt, ttl)), nil
		}
	}()
	var ne *NetError
	var pe *PolicyError
	var re *ResolveError
	var te *TimeoutError
	kind := ""
	switch { // CATCH NetError, PolicyError, ResolveError, Timeout AS e: báo lỗi cho mô hình (AI)
	case errors.As(err, &ne):
		kind = "NetError"
	case errors.As(err, &pe):
		kind = "PolicyError"
	case errors.As(err, &re):
		kind = "ResolveError"
	case errors.As(err, &te):
		kind = "Timeout"
	default:
		return res, err
	}
	logErr("  -> "+kind+":", err)
	return callResult{"complete", true, []content{{"text", err.Error()}}}, nil
}

// dispatch – Mã giả 8.4, DISPATCH(method, p).
func dispatch(method any, rawParams json.RawMessage) (any, error) {
	p := decodeObject(rawParams) // params vắng: {}
	m, _ := p["_meta"].(map[string]any)
	if m == nil {
		m = map[string]any{} // _meta vắng: {}
	}
	v := m["io.modelcontextprotocol/protocolVersion"]
	caps := m["io.modelcontextprotocol/clientCapabilities"]
	if v == nil || caps == nil {
		return nil, &RpcError{-32602, "Invalid params: thiếu _meta"}
	}
	if vs, isStr := v.(string); !isStr || !slices.Contains(SUPPORTED, vs) {
		return nil, &RpcError{-32022, "UnsupportedProtocolVersion"}
	}
	switch method {
	case "server/discover": // DISCOVER_RESULT(SUPPORTED)
		return struct {
			ResultType        string         `json:"resultType"`
			SupportedVersions []string       `json:"supportedVersions"`
			Capabilities      map[string]any `json:"capabilities"`
			ServerInfo        struct {
				Name    string `json:"name"`
				Version string `json:"version"`
			} `json:"serverInfo"`
		}{"complete", SUPPORTED, map[string]any{"tools": map[string]any{}},
			struct {
				Name    string `json:"name"`
				Version string `json:"version"`
			}{"nettools-mcp", "1.0.0"}}, nil
	case "tools/list":
		return struct {
			ResultType string `json:"resultType"`
			Tools      []Tool `json:"tools"`
			TtlMs      int    `json:"ttlMs"`
			CacheScope string `json:"cacheScope"`
		}{"complete", TOOLS, 3600000, "public"}, nil
	case "tools/call":
		var raw struct { // lấy nguyên văn arguments để ghi nhật ký đúng thứ tự khóa
			Arguments json.RawMessage `json:"arguments"`
		}
		json.Unmarshal(rawParams, &raw)
		a, isObj := p["arguments"].(map[string]any)
		rawArgs := "{}"
		if isObj {
			var b bytes.Buffer
			json.Compact(&b, raw.Arguments)
			rawArgs = b.String()
		} else {
			a = map[string]any{}
		}
		return callTool(p["name"], a, rawArgs)
	}
	return nil, &RpcError{-32601, "Method not found"}
}

// safeDispatch: bổ sung – lỗi ngoài dự kiến (panic) khi xử lý một yêu cầu trả -32603 thay vì dừng server.
func safeDispatch(method any, params json.RawMessage) (res any, err error) {
	defer func() {
		if r := recover(); r != nil {
			err = fmt.Errorf("panic: %v", r)
		}
	}()
	return dispatch(method, params)
}

// mcpStdioServer – Mã giả 8.4, MCP_STDIO_SERVER().
func mcpStdioServer() {
	nets := []string{}
	for _, n := range ALLOWED_NETS {
		nets = append(nets, n.String())
	}
	allowed := strings.Join(nets, ", ")
	if allowed == "" {
		allowed = "(rỗng: chặn mọi đích)"
	}
	logErr("MCP server công cụ mạng (stdio), phiên bản", strings.Join(SUPPORTED, ", "), "| ALLOWED_NETS =", allowed)
	r := bufio.NewReaderSize(os.Stdin, 1048576+2)
	for {
		line, err := readLine(r, 1048576) // dòng 8
		if err == io.EOF {
			break
		}
		if err != nil {
			logErr("<-", err)
			break
		}
		if !json.Valid([]byte(line)) { // JsonError
			logErr("<- dòng không phải JSON -> -32700")
			writeMsg(errorMsg{"2.0", nil, RpcError{-32700, "Parse error"}})
			continue
		}
		var msg map[string]json.RawMessage // JSON hợp lệ nhưng không phải đối tượng: coi như không có id
		json.Unmarshal([]byte(line), &msg)
		id := msg["id"]
		var method any
		json.Unmarshal(msg["method"], &method)
		if len(id) == 0 || string(id) == "null" { // thông báo: không trả lời
			logErr(fmt.Sprintf("<- thông báo %v: không trả lời", method))
			continue
		}
		logErr(fmt.Sprintf("<- id=%s %v", id, method))
		res, err := safeDispatch(method, msg["params"])
		var re *RpcError
		switch {
		case err == nil:
			writeMsg(resultMsg{"2.0", id, res})
			logErr(fmt.Sprintf("-> id=%s result", id))
		case errors.As(err, &re):
			writeMsg(errorMsg{"2.0", id, *re})
			logErr(fmt.Sprintf("-> id=%s error %d", id, re.Code))
		default: // bổ sung: lỗi ngoài dự kiến -> -32603
			writeMsg(errorMsg{"2.0", id, RpcError{-32603, "Internal error"}})
			logErr(fmt.Sprintf("-> id=%s error -32603 (%v)", id, err))
		}
	}
	logErr("Host đã đóng stdin, kết thúc")
}

func main() {
	mcpStdioServer()
}

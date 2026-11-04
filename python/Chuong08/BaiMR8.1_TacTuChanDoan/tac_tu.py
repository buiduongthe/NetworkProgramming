"""Bài mở rộng MR8.1 – Tác tử AI chẩn đoán mạng bằng gọi công cụ (tool calling), có lớp kiểm soát.

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 8.1 (gọi API LLM) và Bài 8.3 (MCP server):
Bài 8.3 viết phía server công cụ, bài này viết phía "chủ" (host/agent) – vòng lặp
    hỏi LLM -> LLM yêu cầu công cụ -> chương trình KIỂM TRA rồi mới chạy -> gửi kết quả -> ...
cho tới khi LLM trả lời bằng văn bản. LLM không bao giờ tự chạy lệnh; mọi thao tác mạng do
mã của ta thực hiện, sau khi qua các lớp kiểm soát:
  1. Chỉ chạy công cụ có trong danh mục TOOLS (chặn công cụ "bịa" do ảo giác).
  2. Tham số phải là JSON hợp lệ, đủ trường, đúng kiểu, trong miền giá trị.
  3. Chính sách đích: chỉ máy thuộc mạng phòng thí nghiệm (loopback, mạng riêng RFC 1918).
  4. Giới hạn số vòng lặp; ghi nhật ký kiểm toán từng lời gọi.
Cấu hình qua biến môi trường: LLM_BASE_URL, LLM_API_KEY, LLM_MODEL (như Bài 8.1).

Chạy:  python3 tac_tu.py "<câu hỏi>"
"""
import ipaddress
import json
import os
import socket
import sys
import textwrap
import urllib.error
import urllib.request

BASE_URL = os.environ.get("LLM_BASE_URL", "http://127.0.0.1:9000/v1").rstrip("/")
MODEL = os.environ.get("LLM_MODEL", "mock-model")
MAX_ROUNDS = 4
ALLOWED_NETS = [ipaddress.ip_network(n) for n in ("127.0.0.0/8", "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16")]


class ToolError(Exception):
    """Lời gọi công cụ bị từ chối hoặc thất bại – kết quả gửi lại LLM dưới dạng {"loi": ...}."""


# ---------- Thân công cụ (chạy bằng socket API, không gọi shell) ----------
def resolve(name):
    try:
        return sorted({sa[0] for *_, sa in socket.getaddrinfo(name, None, type=socket.SOCK_STREAM)})
    except socket.gaierror:
        return []


def check_target(host):
    """Chính sách đích: mọi địa chỉ của host phải thuộc ALLOWED_NETS."""
    addrs = resolve(host)
    if not addrs:
        raise ToolError(f"không phân giải được {host}")
    for a in addrs:
        if not any(ipaddress.ip_address(a) in n for n in ALLOWED_NETS):
            raise ToolError(f"{a} nằm ngoài mạng phòng thí nghiệm")
    return addrs


def tool_resolve(args):
    return {"dia_chi": resolve(args["ten"])}


def tool_port(args):
    addrs = check_target(args["host"])
    state = "FILTERED"
    for a in addrs:
        try:
            with socket.create_connection((a, args["cong"]), timeout=args.get("thoi_gian_cho_ms", 1000) / 1000):
                return {"trang_thai": "OPEN", "dia_chi": a}
        except ConnectionRefusedError:
            state = "CLOSED"
        except OSError:
            pass
    return {"trang_thai": state}


# Danh mục công cụ: tên -> (hàm, lược đồ tham số JSON Schema gửi cho LLM)
TOOLS = {
    "phan_giai_ten": (tool_resolve, {
        "type": "object", "required": ["ten"],
        "properties": {"ten": {"type": "string", "description": "tên máy cần phân giải"}}}),
    "kiem_tra_cong": (tool_port, {
        "type": "object", "required": ["host", "cong"],
        "properties": {"host": {"type": "string"},
                       "cong": {"type": "integer", "minimum": 1, "maximum": 65535},
                       "thoi_gian_cho_ms": {"type": "integer", "minimum": 100, "maximum": 5000}}}),
}
DESCRIPTIONS = {"phan_giai_ten": "Phân giải tên máy thành danh sách địa chỉ IP",
                "kiem_tra_cong": "Kiểm tra cổng TCP: OPEN, CLOSED hoặc FILTERED"}


def validate(schema, args):
    """Kiểm tra tham số theo lược đồ (tập con nhỏ của JSON Schema: required, type, min, max)."""
    if not isinstance(args, dict):
        raise ToolError("tham số phải là đối tượng JSON")
    for key in schema["required"]:
        if key not in args:
            raise ToolError(f"thiếu tham số {key}")
    for key, value in args.items():
        spec = schema["properties"].get(key)
        if spec is None:
            raise ToolError(f"tham số lạ {key}")
        if spec["type"] == "integer" and (not isinstance(value, int) or isinstance(value, bool)):
            raise ToolError(f"{key} phải là số nguyên")
        if spec["type"] == "string" and not isinstance(value, str):
            raise ToolError(f"{key} phải là chuỗi")
        if "minimum" in spec and not spec["minimum"] <= value <= spec["maximum"]:
            raise ToolError(f"{key} ngoài miền [{spec['minimum']}, {spec['maximum']}]")


def run_tool(name, arguments):
    """Ba lớp kiểm soát trước khi chạy: danh mục, tham số, chính sách đích (trong thân công cụ)."""
    if name not in TOOLS:
        raise ToolError("công cụ không có trong danh mục")
    try:
        args = json.loads(arguments)
    except ValueError:
        raise ToolError("tham số không phải JSON")
    fn, schema = TOOLS[name]
    validate(schema, args)
    return fn(args)


def compact(obj):
    """JSON gọn, khóa theo thứ tự chữ cái (giống json.Marshal của Go với map)."""
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


# ---------- Gọi API LLM (không truyền luồng) ----------
def chat(messages):
    key = os.environ.get("LLM_API_KEY")
    if not key:
        sys.exit("Thiếu biến môi trường LLM_API_KEY")
    req = {"model": MODEL, "messages": messages, "temperature": 0,
           "tools": [{"type": "function", "function": {"name": n, "description": DESCRIPTIONS[n],
                                                      "parameters": s}} for n, (_, s) in TOOLS.items()]}
    rq = urllib.request.Request(f"{BASE_URL}/chat/completions", method="POST",
                                data=json.dumps(req, ensure_ascii=False).encode("utf-8"),
                                headers={"Content-Type": "application/json", "Authorization": f"Bearer {key}"})
    with urllib.request.urlopen(rq, timeout=10) as resp:
        return json.load(resp)["choices"][0]["message"]


def main():
    question = sys.argv[1] if len(sys.argv) > 1 else "Kiểm tra localhost cổng 80"
    print(f"[tac-tu] Hỏi: {question}", flush=True)
    messages = [{"role": "system", "content": "Bạn là trợ lý chẩn đoán mạng. Chỉ dùng công cụ được cung cấp."},
                {"role": "user", "content": question}]
    for rnd in range(1, MAX_ROUNDS + 1):
        try:
            msg = chat(messages)
        except (urllib.error.URLError, OSError) as e:
            print(f"[tac-tu] Lỗi gọi LLM: {e}", flush=True)
            sys.exit(1)
        messages.append(msg)
        calls = msg.get("tool_calls") or []
        if not calls:
            print(f"[tac-tu] Vòng {rnd}: LLM trả lời", flush=True)
            for line in textwrap.wrap(msg.get("content") or "", 58):
                print("  " + line, flush=True)
            return
        print(f"[tac-tu] Vòng {rnd}: LLM yêu cầu {len(calls)} công cụ", flush=True)
        for c in calls:
            name, arguments = c["function"]["name"], c["function"]["arguments"]
            try:
                result = run_tool(name, arguments)
                print(f"  CHẠY  {name}{arguments} -> {compact(result)}", flush=True)
            except ToolError as e:
                result = {"loi": str(e)}
                print(f"  CHẶN  {name}{arguments}: {e}", flush=True)
            messages.append({"role": "tool", "tool_call_id": c["id"], "content": compact(result)})
    print(f"[tac-tu] Dừng: quá {MAX_ROUNDS} vòng mà chưa có câu trả lời", flush=True)


if __name__ == "__main__":
    main()

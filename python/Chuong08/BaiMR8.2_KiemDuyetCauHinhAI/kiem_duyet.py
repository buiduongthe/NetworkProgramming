"""Bài mở rộng MR8.2 – Kiểm duyệt cấu hình do LLM sinh ra trước khi dùng (guardrail + vòng phản hồi).

Bài mở rộng, không có trong giáo trình; nối tiếp Bài 8.1 (gọi API LLM), Bài 3.3 (dữ liệu JSON)
và Bài MR1.1 (kiểm tra địa chỉ bằng ipaddress). Nguyên tắc: đầu ra của LLM là DỮ LIỆU CHƯA TIN CẬY,
giống dữ liệu nhận từ mạng; phải kiểm tra như kiểm tra gói tin từ client lạ:
  1. Tách và phân tích JSON (LLM hay bọc JSON trong khối ```json kèm lời dẫn).
  2. Lược đồ: đủ trường, đúng kiểu.
  3. Ngữ nghĩa: VLAN 1–4094, không trùng; mạng con hợp lệ, nằm trong khối địa chỉ, không chồng
     lấn nhau và không đụng hạ tầng đang dùng; cổng mặc định là địa chỉ máy hợp lệ trong mạng.
  4. Chính sách an toàn: cấm "permit ip any any", cấm bật telnet.
Nếu có lỗi: gửi danh sách lỗi lại cho LLM để sửa (tối đa --so-lan lần); hết lượt vẫn sai thì
TỪ CHỐI. Cấu hình đạt mới được chuyển thành lệnh CLI – và vẫn cần người duyệt trước khi áp dụng.
Cấu hình qua biến môi trường: LLM_BASE_URL, LLM_API_KEY, LLM_MODEL (như Bài 8.1).

Chạy:  python3 kiem_duyet.py <ha_tang.json> "<yêu cầu>" [--so-lan N]
"""
import argparse
import ipaddress
import json
import os
import re
import sys
import urllib.error
import urllib.request

BASE_URL = os.environ.get("LLM_BASE_URL", "http://127.0.0.1:9000/v1").rstrip("/")
MODEL = os.environ.get("LLM_MODEL", "mock-model")
SYSTEM = ("Bạn sinh cấu hình mạng. Chỉ trả về một đối tượng JSON có các khóa: "
          "vlan (danh sách {id, ten, mang, cong_mac_dinh}), acl (danh sách {ten, quy_tac}), "
          "dich_vu ({telnet, ssh}).")
FORBIDDEN_RULES = re.compile(r"permit\s+(ip\s+)?any\s+any")


def chat(messages):
    key = os.environ.get("LLM_API_KEY")
    if not key:
        sys.exit("Thiếu biến môi trường LLM_API_KEY")
    req = {"model": MODEL, "messages": messages, "temperature": 0,
           "response_format": {"type": "json_object"}}
    rq = urllib.request.Request(f"{BASE_URL}/chat/completions", method="POST",
                                data=json.dumps(req, ensure_ascii=False).encode("utf-8"),
                                headers={"Content-Type": "application/json", "Authorization": f"Bearer {key}"})
    with urllib.request.urlopen(rq, timeout=10) as resp:
        return json.load(resp)["choices"][0]["message"]["content"]


def extract_json(text):
    """Lấy JSON từ khối ```json ... ``` nếu có, nếu không thì dùng toàn bộ văn bản."""
    m = re.search(r"```(?:json)?\s*\n(.*?)```", text, re.S)
    return json.loads(m.group(1) if m else text)


def check_schema(cfg):
    errs = []
    if not isinstance(cfg, dict):
        return ["gốc phải là đối tượng JSON"]
    for key, typ in (("vlan", list), ("acl", list), ("dich_vu", dict)):
        if not isinstance(cfg.get(key), typ):
            errs.append(f"thiếu khóa {key} hoặc sai kiểu")
    for i, v in enumerate(cfg.get("vlan") or []):
        for key, typ in (("id", int), ("ten", str), ("mang", str), ("cong_mac_dinh", str)):
            if not isinstance(v.get(key), typ) or (typ is int and isinstance(v.get(key), bool)):
                errs.append(f"vlan[{i}].{key} thiếu hoặc sai kiểu")
    # quy_tac phải là danh sách chuỗi: nếu là một chuỗi, vòng lặp kiểm chính sách sẽ duyệt từng
    # ký tự và quy tắc cấm (ví dụ "permit ip any any") lọt qua.
    for i, a in enumerate(cfg["acl"] if isinstance(cfg.get("acl"), list) else []):
        a = a if isinstance(a, dict) else {}
        if not isinstance(a.get("ten"), str):
            errs.append(f"acl[{i}].ten thiếu hoặc sai kiểu")
        q = a.get("quy_tac")
        if not (isinstance(q, list) and all(isinstance(r, str) for r in q)):
            errs.append(f"acl[{i}].quy_tac thiếu hoặc sai kiểu")
    return errs


def check_semantics(cfg, infra):
    errs, nets = [], []
    block = ipaddress.ip_network(infra["khoi_dia_chi"])
    used_ids = {d["vlan"]: d["ten"] for d in infra["da_dung"]}
    seen_ids = set()
    for v in cfg["vlan"]:
        name = v["ten"]
        if not 1 <= v["id"] <= 4094:
            errs.append(f"{name}: VLAN {v['id']} ngoài miền 1-4094")
        elif v["id"] in used_ids:
            errs.append(f"{name}: VLAN {v['id']} đã dùng cho {used_ids[v['id']]}")
        elif v["id"] in seen_ids:
            errs.append(f"{name}: VLAN {v['id']} bị trùng")
        seen_ids.add(v["id"])
        try:
            net = ipaddress.ip_network(v["mang"])               # strict: không chấp nhận bit máy
        except ValueError:
            errs.append(f"{name}: mạng {v['mang']} không hợp lệ")
            continue
        if not net.subnet_of(block):
            errs.append(f"{name}: mạng {net} ngoài khối {block}")
        try:
            gw = ipaddress.ip_address(v["cong_mac_dinh"])
            if gw not in net or gw in (net.network_address, net.broadcast_address):
                errs.append(f"{name}: cổng mặc định {gw} không phải địa chỉ máy của {net}")
        except ValueError:
            errs.append(f"{name}: cổng mặc định {v['cong_mac_dinh']} không phải IPv4 hợp lệ")
        for other, onet in nets:
            if net.overlaps(onet):
                errs.append(f"{name}: mạng {net} chồng lấn {other} ({onet})")
        for d in infra["da_dung"]:
            if net.overlaps(ipaddress.ip_network(d["mang"])):
                errs.append(f"{name}: mạng {net} đụng hạ tầng {d['ten']} ({d['mang']})")
        nets.append((name, net))
    return errs


def check_policy(cfg):
    errs = []
    for acl in cfg["acl"]:
        for rule in acl.get("quy_tac", []):
            if FORBIDDEN_RULES.search(rule):
                errs.append(f"ACL {acl.get('ten')}: cấm quy tắc \"{rule}\"")
    if cfg["dich_vu"].get("telnet") is not False:
        errs.append("dich_vu.telnet phải tắt (mật khẩu đi dạng rõ), dùng SSH")
    return errs


def validate(text, infra):
    try:
        cfg = extract_json(text)
    except ValueError as e:
        return None, [f"JSON hỏng: {e}"]
    errs = check_schema(cfg)
    if errs:
        return None, errs
    return cfg, check_semantics(cfg, infra) + check_policy(cfg)


def render(cfg):
    """Chuyển cấu hình đã duyệt thành lệnh CLI (kiểu IOS) – mẫu đơn giản."""
    lines = []
    for v in cfg["vlan"]:
        net = ipaddress.ip_network(v["mang"])
        lines += [f"vlan {v['id']}", f" name {v['ten']}", f"interface Vlan{v['id']}",
                  f" ip address {v['cong_mac_dinh']} {net.netmask}"]
    for acl in cfg["acl"]:
        lines.append(f"ip access-list extended {acl['ten']}")
        lines += [f" {r}" for r in acl["quy_tac"]]
    lines.append("no service telnet" if not cfg["dich_vu"]["telnet"] else "service telnet")
    return lines


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("ha_tang")
    ap.add_argument("yeu_cau")
    ap.add_argument("--so-lan", type=int, default=3)
    a = ap.parse_args()
    infra = json.load(open(a.ha_tang, encoding="utf-8"))
    messages = [{"role": "system", "content": SYSTEM}, {"role": "user", "content": a.yeu_cau}]
    print(f"[kiem-duyet] Yêu cầu: {a.yeu_cau}", flush=True)
    for attempt in range(1, a.so_lan + 1):
        try:
            text = chat(messages)
        except (urllib.error.URLError, OSError) as e:
            print(f"[kiem-duyet] Lỗi gọi LLM: {e}", flush=True)
            sys.exit(1)
        messages.append({"role": "assistant", "content": text})
        cfg, errs = validate(text, infra)
        if not errs:
            print(f"[kiem-duyet] Lần {attempt}: ĐẠT", flush=True)
            print("[kiem-duyet] Lệnh CLI sinh từ cấu hình đã duyệt (chờ người duyệt):", flush=True)
            for line in render(cfg):
                print("  " + line, flush=True)
            return
        print(f"[kiem-duyet] Lần {attempt}: {len(errs)} lỗi", flush=True)
        for e in errs:
            print(f"  - {e}", flush=True)
        messages.append({"role": "user", "content": "Cấu hình chưa đạt, hãy sửa và chỉ trả về JSON:\n"
                         + "\n".join(f"- {e}" for e in errs)})
    print(f"[kiem-duyet] TỪ CHỐI: sau {a.so_lan} lần vẫn chưa đạt, không sinh lệnh", flush=True)
    sys.exit(1)


if __name__ == "__main__":
    main()

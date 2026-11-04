"""Bài 6.1 – Nghiệp vụ tra cứu thuật ngữ, chép gọn từ Mã giả 3.7 (Mục 3.5.1).

PROCESS_REQUEST độc lập với socket; HANDLE_LOOKUP là vòng phục vụ một client.
Cả hai server của bài (thread_server.py, pool_server.py) dùng lại nguyên vẹn.
"""
import json
import os

from netutil import ConnectionClosed, recv_json, send_json

_HERE = os.path.dirname(os.path.abspath(__file__))
with open(os.path.join(_HERE, "glossary.json"), encoding="utf-8") as f:
    DICT = json.load(f)                                  # LOAD_JSON_FILE("glossary.json")


def process_request(req):
    """Mã giả 3.7, dòng 2–12: ánh xạ thao tác → phản hồi."""
    op = req.get("op")
    if op == "lookup":
        term = req.get("term")
        if term in DICT:
            return {"status": 200, "term": term, "text": DICT[term]}
        return {"status": 404, "error": "Không có thuật ngữ"}
    if op == "list":
        return {"status": 200, "terms": list(DICT)}
    return {"status": 400, "error": "Sai thao tác"}


def handle_lookup(conn):
    """Mã giả 3.7, dòng 14–27: nhận yêu cầu, trả phản hồi đến khi client gửi bye hoặc đóng."""
    while True:
        try:
            req = recv_json(conn)
        except ConnectionClosed as e:
            if e.received == 0:                          # đóng tại ranh giới khung
                return
            raise                                        # đóng giữa khung: lỗi
        except ValueError:                               # JsonError (gồm lỗi giải mã UTF-8)
            send_json(conn, {"status": 400, "error": "JSON sai"})
            continue
        if req.get("op") == "bye":
            return
        send_json(conn, process_request(req))

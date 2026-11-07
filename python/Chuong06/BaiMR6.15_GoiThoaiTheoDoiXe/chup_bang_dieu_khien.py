"""Bài mở rộng MR6.15 – Chụp ảnh trang bảng điều khiển máy chủ bằng trình duyệt không giao diện (Playwright).

Dùng trong kịch bản demo: chờ --sau giây (lúc cuộc gọi đang diễn ra, tài xế đang chạy) rồi chụp cả trang.
Cần: pip install playwright && python -m playwright install chromium

Chạy:  python chup_bang_dieu_khien.py <url> <tệp_ảnh> [--sau 8] [--rong 1280]
"""
import argparse
import time

from playwright.sync_api import sync_playwright


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("url")
    ap.add_argument("anh")
    ap.add_argument("--sau", type=float, default=8)
    ap.add_argument("--rong", type=int, default=1280)
    a = ap.parse_args()
    t0 = time.time()
    with sync_playwright() as p:
        b = p.chromium.launch()
        trang = b.new_page(viewport={"width": a.rong, "height": 900}, device_scale_factor=1)
        trang.goto(a.url)
        time.sleep(max(0.0, a.sau - (time.time() - t0)))
        trang.wait_for_timeout(300)
        trang.screenshot(path=a.anh, full_page=True)
        so = trang.locator("#kpi b").all_inner_texts()
        b.close()
    print(f"[chup] {time.time() - t0:5.1f}s đã chụp {a.url} -> {a.anh}; số liệu: {', '.join(so)}", flush=True)


if __name__ == "__main__":
    main()
